use super::{commands, state::Explorer};
use crate::{
    operations,
    places::{self, Place},
};
use dioxus_native::prelude::{dioxus_elements::input_data::MouseButton, *};
use std::{path::PathBuf, time::Duration};

#[component]
pub fn Sidebar() -> Element {
    let state = use_context::<Explorer>();
    let mut places = use_signal(places::load);
    // Mounts and GTK bookmarks change outside the app; re-reading them is cheap.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(4)).await;
            let fresh = places::load();
            if *places.peek() != fresh {
                places.set(fresh);
            }
        }
    });
    let places = places.read();
    let current = state.current();
    rsx! {
        aside { class: "sidebar", aria_label: "Places",
            div { class: "side-label", "Quick access" }
            if let Some(home) = places.home.clone() {
                PlaceLink { place: home, current: current.clone() }
            }
            for place in places.folders.iter().cloned() {
                PlaceLink { key: "{place.path.display()}", place, current: current.clone(), nested: true }
            }
            if !places.bookmarks.is_empty() {
                div { class: "side-gap" }
                for place in places.bookmarks.iter().cloned() {
                    PlaceLink { key: "b{place.path.display()}", place, current: current.clone() }
                }
            }
            div { class: "side-gap" }
            PlaceLink { place: Place { name: "File system".into(), path: PathBuf::from("/") }, current: current.clone(), quiet: true }
            for place in places.devices.iter().cloned() {
                PlaceLink { key: "d{place.path.display()}", place, current: current.clone(), quiet: true }
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
) -> Element {
    let state = use_context::<Explorer>();
    let active = place.path == current;
    let class = format!(
        "place{}{}{}",
        if nested { " nested" } else { "" },
        if quiet { " quiet" } else { "" },
        if active { " active" } else { "" },
    );
    let path = place.path.clone();
    let tab_path = place.path.clone();
    rsx! {
        div { class, title: "{place.path.display()}",
            onclick: move |_| state.visit(path.clone()),
            onmousedown: move |event| {
                if event.trigger_button() == Some(MouseButton::Auxiliary) {
                    commands::open_tab(state, tab_path.clone());
                }
            },
            "{place.name}"
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
