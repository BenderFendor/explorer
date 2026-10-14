//! Owns the window event loop. `dioxus_native::launch_cfg` hides its application,
//! and Blitz 0.2 sends no wheel or resize DOM events, so this shell forwards window
//! size and wheel input to the UI, which renders a virtual viewport from them.

use anyrender::WindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent, BlitzShellNetCallback, View, WindowConfig};
use blitz_traits::events::{BlitzKeyEvent, KeyState, UiEvent};
use dioxus_native::{
    DioxusDocument, DioxusNativeWindowRenderer, DocumentConfig, WindowAttributes,
    prelude::{Code, Element, Key, Location, Modifiers, VirtualDom},
};
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use winit::{
    application::ApplicationHandler,
    event::{MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    window::WindowId,
};

/// Logical pixels scrolled per wheel notch.
const LINE: f64 = 56.0;

#[derive(Clone, Debug, PartialEq)]
pub enum ShellEvent {
    Resize {
        width: f64,
        height: f64,
    },
    /// `zoom` is set when Ctrl is held, as in Thunar's Ctrl+wheel zoom.
    Wheel {
        delta: f64,
        x: f64,
        y: f64,
        zoom: bool,
    },
    /// Files dragged from any client are over the window, in logical pixels.
    DragOver {
        x: f64,
        y: f64,
    },
    DragLeave,
    Drop {
        x: f64,
        y: f64,
        paths: Vec<std::path::PathBuf>,
    },
}

type Channel = (
    UnboundedSender<ShellEvent>,
    Mutex<Option<UnboundedReceiver<ShellEvent>>>,
);
static CHANNEL: OnceLock<Channel> = OnceLock::new();

fn channel() -> &'static Channel {
    CHANNEL.get_or_init(|| {
        let (sender, receiver) = unbounded_channel();
        (sender, Mutex::new(Some(receiver)))
    })
}

pub fn publish(event: ShellEvent) {
    let _ = channel().0.send(event);
}

/// Hands the event stream to the UI once. Screenshot and test documents get one too;
/// it stays silent because no window exists.
pub fn take_events() -> Option<UnboundedReceiver<ShellEvent>> {
    channel().1.lock().ok()?.take()
}

static CARET_TO_END: AtomicBool = AtomicBool::new(false);

/// Parley keeps the caret at 0 when a field mounts with text, so typing would prepend.
/// Prefilled fields call this; the shell then sends the focused field an End key.
pub fn request_caret_at_end() {
    CARET_TO_END.store(true, Ordering::Relaxed);
}

/// Applies a pending caret request to `document`. Returns true when it sent a key.
pub fn apply_caret_request(document: &mut dyn blitz_dom::Document) -> bool {
    if !CARET_TO_END.swap(false, Ordering::Relaxed) {
        return false;
    }
    document.handle_ui_event(key_down(Key::End, Modifiers::empty()));
    true
}

/// A key press as the window would deliver it; used for synthetic input.
pub fn key_down(key: Key, modifiers: Modifiers) -> UiEvent {
    let text = match &key {
        Key::Character(text) => Some(text.as_str().into()),
        _ => None,
    };
    UiEvent::KeyDown(BlitzKeyEvent {
        key,
        code: Code::Unidentified,
        modifiers,
        location: Location::Standard,
        is_auto_repeating: false,
        is_composing: false,
        state: KeyState::Pressed,
        text,
    })
}

/// How the window is painted. Both use Vello; only the rasterizer differs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Backend {
    /// Software rasterizer into a shared-memory buffer. No GPU driver is loaded.
    Cpu,
    /// wgpu on the graphics card.
    Gpu,
}

pub fn launch(
    app: fn() -> Element,
    attributes: WindowAttributes,
    backend: Backend,
) -> Result<(), String> {
    match backend {
        Backend::Cpu => run(app, attributes, crate::raster::CpuWindowRenderer::new()),
        Backend::Gpu => run(app, attributes, DioxusNativeWindowRenderer::new()),
    }
}

fn run<R: WindowRenderer + 'static>(
    app: fn() -> Element,
    attributes: WindowAttributes,
    renderer: R,
) -> Result<(), String> {
    let event_loop = blitz_shell::create_default_event_loop::<BlitzShellEvent>();
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let _guard = runtime.enter();
    let callback = BlitzShellNetCallback::shared(event_loop.create_proxy());
    let document = DioxusDocument::new(
        VirtualDom::new(app),
        DocumentConfig {
            net_provider: Some(blitz_net::Provider::shared(callback)),
            ..Default::default()
        },
    );
    let config = WindowConfig::with_attributes(Box::new(document) as _, renderer, attributes);
    let mut shell = Shell {
        pending: Some(config),
        inner: BlitzApplication::new(event_loop.create_proxy()),
    };
    event_loop
        .run_app(&mut shell)
        .map_err(|error| error.to_string())
}

struct Shell<R: WindowRenderer> {
    pending: Option<WindowConfig<R>>,
    inner: BlitzApplication<R>,
}

fn publish_size<R: WindowRenderer>(view: &View<R>) {
    let size = view
        .window
        .inner_size()
        .to_logical::<f64>(view.window.scale_factor());
    publish(ShellEvent::Resize {
        width: size.width,
        height: size.height,
    });
}

impl<R: WindowRenderer> ApplicationHandler<BlitzShellEvent> for Shell<R> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(config) = self.pending.take() {
            let mut view = View::init(config, event_loop, &self.inner.proxy);
            view.downcast_doc_mut::<DioxusDocument>().initial_build();
            view.request_redraw();
            publish_size(&view);
            if let Err(error) = crate::dnd::attach(&view.window) {
                eprintln!("Explorer: drag and drop unavailable: {error}");
            }
            self.inner.windows.insert(view.window_id(), view);
        }
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.suspended(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(view) = self.inner.windows.get(&id) {
            match &event {
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    publish_size(view)
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let delta = match delta {
                        MouseScrollDelta::LineDelta(_, lines) => f64::from(*lines) * LINE,
                        MouseScrollDelta::PixelDelta(pixels) => {
                            pixels.y / view.window.scale_factor()
                        }
                    };
                    let (x, y) = view.mouse_pos;
                    publish(ShellEvent::Wheel {
                        delta: -delta,
                        x: f64::from(x),
                        y: f64::from(y),
                        zoom: view.keyboard_modifiers.state().control_key(),
                    });
                }
                _ => {}
            }
        }
        self.inner.window_event(event_loop, id, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: BlitzShellEvent) {
        self.inner.user_event(event_loop, event);
        for view in self.inner.windows.values_mut() {
            if apply_caret_request(view.doc.as_mut()) {
                view.request_redraw();
            }
        }
    }
}
