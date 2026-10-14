use super::{commands, state::Explorer};
use crate::{
    operations,
    places::{self, Place, Places},
};
use dioxus_native::prelude::{dioxus_elements::input_data::MouseButton, *};
use std::{path::PathBuf, time::Duration};

#[component]
pub fn Sidebar() -> Element {
    let state = use_context::<Explorer>();
    let mut places = use_signal(places::load);
    let mut expanded = use_signal(|| false);
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let fresh = places::load();
            if *places.peek() != fresh {
                places.set(fresh);
            }
        }
    });
    let shown = places.read();
    let current = state.current();
    rsx! {
        aside { class: "sidebar", aria_label: "Places",
            div { class: "side-label", "Places" }
            if let Some(home) = shown.home.clone() {
                PlaceLink { place: home, current: current.clone() }
            }
            for place in shown.folders.iter().cloned() {
                PlaceLink { key: "f{place.path.display()}", place, current: current.clone(), nested: true }
            }
            div { class: "side-gap" }
            div { class: "side-heading",
                span { "Pinned" }
                span { class: "side-action", title: "Pin the open folder",
                    onclick: move |_| {
                        match places::pin(&state.current()) {
                            Ok(()) => { places.set(places::load()); state.notify("Folder pinned", false); }
                            Err(error) => state.notify(error, true),
                        }
                    },
                    "+"
                }
            }
            for place in shown.bookmarks.iter().cloned() {
                PlaceLink { key: "b{place.path.display()}", place, current: current.clone(), pinned: true, reload: Some(places) }
            }
            div { class: "side-gap" }
            div { class: "quick-access",
                div { class: "side-heading quick-toggle",
                    onclick: move |_| expanded.toggle(),
                    span { if (expanded)() { "⌄ Quick access" } else { "› Quick access" } }
                }
                div { class: if (expanded)() { "quick-items expanded" } else { "quick-items" },
                    if !shown.frequent.is_empty() {
                        div { class: "side-subheading", "Frequent folders" }
                        for place in shown.frequent.iter().cloned() {
                            PlaceLink { key: "q{place.path.display()}", place, current: current.clone(), nested: true }
                        }
                    }
                    if !shown.recent.is_empty() {
                        div { class: "side-subheading", "Recent files" }
                        for place in shown.recent.iter().cloned() {
                            PlaceLink { key: "r{place.path.display()}", place, current: current.clone(), nested: true }
                        }
                    }
                }
            }
            div { class: "side-gap" }
            PlaceLink { place: Place { name: "File system".into(), path: PathBuf::from("/") }, current: current.clone(), quiet: true }
            for place in shown.devices.iter().cloned() {
                if !shown.mountable.iter().any(|device| device.mount.as_ref()==Some(&place.path)) {
                    PlaceLink { key: "d{place.path.display()}", place, current: current.clone(), quiet: true }
                }
            }
            for device in shown.mountable.iter().cloned() {
                DeviceLink { key: "dev{device.block.display()}", device, refresh: places }
            }
            PlaceLink { place: Place { name: "Trash".into(), path: operations::trash_files() }, current: current.clone(), quiet: true }
            Preferences {}
        }
    }
}

#[component]
fn PlaceLink(
    place: Place,
    current: PathBuf,
    #[props(default)] nested: bool,
    #[props(default)] quiet: bool,
    #[props(default)] pinned: bool,
    #[props(default)] reload: Option<Signal<Places>>,
) -> Element {
    let state = use_context::<Explorer>();
    let active = place.path == current;
    let class = format!(
        "place{}{}{}",
        if nested { " nested" } else { "" },
        if quiet { " quiet" } else { "" },
        if active { " active" } else { "" }
    );
    let path = place.path.clone();
    let tab_path = place.path.clone();
    let unpin_path = place.path.clone();
    rsx! {
        div { class, title: "{place.path.display()}",
            onclick: move |_| {
                if path.is_dir() { state.visit(path.clone()); }
                else {
                    crate::history::record(&path, false);
                    if let Err(error) = operations::open(&path) { state.notify(error, true); }
                }
            },
            onmousedown: move |event| {
                if event.trigger_button() == Some(MouseButton::Auxiliary) && tab_path.is_dir() {
                    commands::open_tab(state, tab_path.clone());
                }
            },
            span { class: "place-title", "{place.name}" }
            if pinned {
                span { class: "place-remove", title: "Unpin folder",
                    onclick: move |event| {
                        event.stop_propagation();
                        if let Err(error) = places::unpin(&unpin_path) { state.notify(error, true); }
                        else if let Some(mut reload) = reload { reload.set(places::load()); }
                    },
                    "×"
                }
            }
        }
    }
}

#[component]
fn DeviceLink(device:crate::devices::Device,refresh:Signal<Places>)->Element {
    let mut state=use_context::<Explorer>();
    let mut refresh=refresh;
    let block=device.block.clone();
    let block_action=device.block.clone();
    let mount=device.mount.clone();
    rsx! {
        div {class:"place quiet",title:"{device.block.display()}",
            span {class:"place-title",
                onclick:move |_|{
                    if let Some(path)=mount.clone(){state.visit(path);}
                    else {match crate::devices::mount(&block){
                        Ok(path)=>{refresh.set(places::load());state.visit(path);}
                        Err(e)=>state.notify(e,true)
                    }}
                },
                "{device.label}"
            }
            span {class:"side-action",title:if device.mount.is_some(){"Unmount device"}else{"Mount device"},
                onclick:move |event|{
                    event.stop_propagation();
                    let result=if device.mount.is_some(){crate::devices::unmount(&block_action)}else{crate::devices::mount(&block_action).map(|_|())};
                    match result {Ok(())=>refresh.set(places::load()),Err(e)=>state.notify(e,true)}
                },
                if device.mount.is_some(){"⏏"}else{"+"}
            }
        }
    }
}
#[component]
fn Preferences() -> Element {
    let mut state = use_context::<Explorer>();
    rsx! {
        div { class: "preferences",
            span { class: "preference", title: "Cycle Matugen, light, and dark",
                onclick: move |_| { let next = (state.theme)().next(); state.theme.set(next); },
                "{(state.theme)().name()}"
            }
            span { class: "preference", title: "Toggle animations",
                onclick: move |_| state.motion.toggle(),
                if (state.motion)() { "motion" } else { "still" }
            }
            span { class: "preference", title: "Keyboard shortcuts (?)",
                onclick: move |_| state.overlay.set(super::state::Overlay::Help),
                "keys"
            }
        }
    }
}
