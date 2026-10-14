//! Drag and drop with other Wayland clients. winit 0.30 has no Wayland drag and drop,
//! so this module opens a second event queue on winit's own `wl_display` (libwayland
//! supports that) and uses the core `wl_data_device` protocol on the window's surface.

use crate::{clipboard, operations::Transfer, shell};
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::fd::AsFd,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    backend::{Backend, ObjectId},
    event_created_child,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_data_device::{self, WlDataDevice},
        wl_data_device_manager::{DndAction, WlDataDeviceManager},
        wl_data_offer::{self, WlDataOffer},
        wl_data_source::{self, WlDataSource},
        wl_pointer::{self, ButtonState, WlPointer},
        wl_registry::WlRegistry,
        wl_seat::{self, WlSeat},
        wl_surface::WlSurface,
    },
};
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

const URI_LIST: &str = "text/uri-list";
const GNOME: &str = "x-special/gnome-copied-files";

/// Objects the UI thread needs to start a drag. Wayland proxies are thread-safe.
struct Handles {
    connection: Connection,
    queue: QueueHandle<Dnd>,
    manager: WlDataDeviceManager,
    device: Option<WlDataDevice>,
    surface: WlSurface,
    /// Serial of the latest button press on our surface; a drag must cite it.
    press: Option<u32>,
}

static HANDLES: OnceLock<Mutex<Handles>> = OnceLock::new();

struct Dnd {
    surface: ObjectId,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    over_surface: bool,
    offers: HashMap<ObjectId, Vec<String>>,
    hovered: Option<(WlDataOffer, f64, f64)>,
}

/// Attaches drag and drop to a Wayland window. Other platforms keep in-app operations only.
pub fn attach(window: &winit::window::Window) -> Result<(), String> {
    let (Ok(display), Ok(handle)) = (window.display_handle(), window.window_handle()) else {
        return Err("no window handles".into());
    };
    let (RawDisplayHandle::Wayland(display), RawWindowHandle::Wayland(handle)) =
        (display.as_raw(), handle.as_raw())
    else {
        return Ok(());
    };
    // SAFETY: winit owns this wl_display and keeps it alive for the window's lifetime,
    // which is the lifetime of the process here.
    let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
    let connection = Connection::from_backend(backend);
    let (globals, queue) =
        registry_queue_init::<Dnd>(&connection).map_err(|error| error.to_string())?;
    let handle_queue = queue.handle();
    // SAFETY: the pointer is winit's live wl_surface on the same display.
    let surface_id =
        unsafe { ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()) }
            .map_err(|error| error.to_string())?;
    let surface =
        WlSurface::from_id(&connection, surface_id.clone()).map_err(|error| error.to_string())?;
    let manager: WlDataDeviceManager = globals
        .bind(&handle_queue, 1..=3, ())
        .map_err(|error| error.to_string())?;
    let seat: WlSeat = globals
        .bind(&handle_queue, 1..=7, ())
        .map_err(|error| error.to_string())?;
    let device = manager.get_data_device(&seat, &handle_queue, ());
    let _ = HANDLES.set(Mutex::new(Handles {
        connection: connection.clone(),
        queue: handle_queue,
        manager,
        device: Some(device),
        surface,
        press: None,
    }));
    let state = Dnd {
        surface: surface_id,
        seat: Some(seat),
        pointer: None,
        over_surface: false,
        offers: HashMap::new(),
        hovered: None,
    };
    std::thread::Builder::new()
        .name("dnd".into())
        .spawn(move || run(queue, state))
        .map(drop)
        .map_err(|error| error.to_string())
}

fn run(mut queue: EventQueue<Dnd>, mut state: Dnd) {
    while queue.blocking_dispatch(&mut state).is_ok() {}
}

/// Starts dragging `paths` out of the window. Call while the mouse button is held.
pub fn start_drag(paths: &[PathBuf]) -> bool {
    let Some(handles) = HANDLES.get() else {
        return false;
    };
    let Ok(handles) = handles.lock() else {
        return false;
    };
    let (Some(device), Some(serial)) = (&handles.device, handles.press) else {
        return false;
    };
    let data = Arc::new(clipboard::encode(paths, Transfer::Copy));
    let source = handles.manager.create_data_source(&handles.queue, data);
    source.offer(URI_LIST.into());
    source.offer(GNOME.into());
    source.offer("text/plain;charset=utf-8".into());
    if source.version() >= 3 {
        source.set_actions(DndAction::Copy | DndAction::Move);
    }
    device.start_drag(Some(&source), &handles.surface, None, serial);
    handles.connection.flush().is_ok()
}

impl Dnd {
    /// Accepts file lists only; anything else shows the compositor's "no drop" cursor.
    fn enter(&mut self, offer: WlDataOffer, serial: u32, x: f64, y: f64) {
        let files = self
            .offers
            .get(&offer.id())
            .is_some_and(|types| types.iter().any(|t| t == URI_LIST));
        offer.accept(serial, files.then(|| URI_LIST.to_owned()));
        if !files {
            return;
        }
        if offer.version() >= 3 {
            // The drop target picks copy or move itself, as Thunar does.
            offer.set_actions(DndAction::Copy | DndAction::Move, DndAction::Move);
        }
        shell::publish(shell::ShellEvent::DragOver { x, y });
        self.hovered = Some((offer, x, y));
    }
}

fn publish_drop(offer: WlDataOffer, x: f64, y: f64, connection: Connection) {
    let (mut reader, writer) = match std::io::pipe() {
        Ok(pipe) => pipe,
        Err(_) => return,
    };
    offer.receive(URI_LIST.into(), writer.as_fd());
    drop(writer);
    let _ = connection.flush();
    // Our own drags are served by this module's thread, so read elsewhere.
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        if offer.version() >= 3 {
            offer.finish();
        }
        offer.destroy();
        let _ = connection.flush();
        let paths = clipboard::decode_uri_list(&text);
        if !paths.is_empty() {
            shell::publish(shell::ShellEvent::Drop { x, y, paths });
        }
    });
}

impl Dispatch<WlRegistry, GlobalListContents> for Dnd {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDeviceManager, ()> for Dnd {
    fn event(
        _: &mut Self,
        _: &WlDataDeviceManager,
        _: <WlDataDeviceManager as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for Dnd {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: wayland_client::WEnum::Value(capabilities),
        } = event
            && capabilities.contains(wl_seat::Capability::Pointer)
            && state.pointer.is_none()
            && state.seat.as_ref() == Some(seat)
        {
            state.pointer = Some(seat.get_pointer(queue, ()));
        }
    }
}

impl Dispatch<WlPointer, ()> for Dnd {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                state.over_surface = surface.id() == state.surface
            }
            wl_pointer::Event::Leave { .. } => state.over_surface = false,
            wl_pointer::Event::Button {
                serial,
                state: wayland_client::WEnum::Value(ButtonState::Pressed),
                ..
            } if state.over_surface => {
                if let Some(Ok(mut handles)) = HANDLES.get().map(Mutex::lock) {
                    handles.press = Some(serial);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, ()> for Dnd {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        connection: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                state.offers.insert(id.id(), Vec::new());
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id: Some(offer),
            } if surface.id() == state.surface => state.enter(offer, serial, x, y),
            wl_data_device::Event::Motion { x, y, .. } => {
                if let Some(hovered) = &mut state.hovered {
                    (hovered.1, hovered.2) = (x, y);
                    shell::publish(shell::ShellEvent::DragOver { x, y });
                }
            }
            wl_data_device::Event::Leave => {
                if let Some((offer, ..)) = state.hovered.take() {
                    state.offers.remove(&offer.id());
                    offer.destroy();
                }
                shell::publish(shell::ShellEvent::DragLeave);
            }
            wl_data_device::Event::Drop => {
                if let Some((offer, x, y)) = state.hovered.take() {
                    state.offers.remove(&offer.id());
                    publish_drop(offer, x, y, connection.clone());
                }
                shell::publish(shell::ShellEvent::DragLeave);
            }
            wl_data_device::Event::Selection { id: Some(offer) } => {
                // The clipboard is read through data-control instead.
                state.offers.remove(&offer.id());
                offer.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(Dnd, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataOffer, ()> for Dnd {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event
            && let Some(types) = state.offers.get_mut(&offer.id())
        {
            types.push(mime_type);
        }
    }
}

impl Dispatch<WlDataSource, Arc<clipboard::Encoded>> for Dnd {
    fn event(
        _: &mut Self,
        source: &WlDataSource,
        event: wl_data_source::Event,
        data: &Arc<clipboard::Encoded>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let body = match mime_type.as_str() {
                    URI_LIST => &data.uri_list,
                    GNOME => &data.gnome,
                    _ => &data.text,
                };
                let _ = std::fs::File::from(fd).write_all(body.as_bytes());
            }
            wl_data_source::Event::Cancelled | wl_data_source::Event::DndFinished => {
                source.destroy()
            }
            _ => {}
        }
    }
}
