mod commands;
mod drag;
mod keyboard;
mod keymap;
mod overlay;
mod preview;
mod sidebar;
mod state;
mod topbar;
mod viewport;
mod views;

use crate::{
    filesystem::{self, Navigation, Order, Sort},
    prefs,
    shell::{self, ShellEvent},
    theme::{self, Theme},
};
use dioxus_native::prelude::{dioxus_elements::input_data::MouseButton, *};
use state::{Cursor, Explorer, Overlay};
use std::{collections::HashMap, path::PathBuf, time::Duration};
use viewport::{View, Window};

pub fn app() -> Element {
    let initial =
        use_hook(|| try_consume_context::<PathBuf>().unwrap_or_else(filesystem::initial_path));
    let navigation = use_signal(|| Navigation::new(initial.clone()));
    let revision = use_signal(|| 0_u64);
    let query = use_signal(String::new);
    let hidden = use_signal(|| true);
    let order = use_signal(|| {
        prefs::load_order().unwrap_or(Order {
            sort: Sort::Modified,
            reverse: false,
        })
    });
    let listing = use_resource(move || {
        let path = navigation.read().current().to_path_buf();
        let _ = revision();
        async move {
            tokio::task::spawn_blocking(move || filesystem::read_directory(&path))
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
        }
    });
    let view = use_signal(View::default);
    let entries = use_memo(move || {
        let current = navigation.read().current().to_path_buf();
        match &*listing.read() {
            // A refresh keeps showing the previous listing until the new one arrives.
            Some(Ok(listing)) if listing.path == current => {
                let mut visible = filesystem::visible_entries(listing, &query(), hidden(), order());
                if view() == View::Gallery {
                    visible.retain(crate::thumbnails::is_image);
                }
                visible
            }
            _ => Vec::new(),
        }
    });
    let state = Explorer {
        navigation,
        tabs: use_signal(|| vec![Navigation::new(initial.clone())]),
        tab: use_signal(|| 0),
        entries,
        query,
        filtering: use_signal(|| false),
        hidden,
        order,
        view,
        gallery_previous: use_signal(View::default),
        gallery_focus: use_signal(|| false),
        gallery_zoom: use_signal(|| 1.0),
        gallery_info: use_signal(|| true),
        zoom: use_signal(|| 1.0),
        cursor: use_signal(|| Cursor {
            path: crate::cli::options().select.clone(),
            index: 0,
        }),
        marks: use_signal(Default::default),
        clipboard: use_signal(|| None),
        remembered: use_signal(HashMap::new),
        bookmarks: use_signal(HashMap::new),
        thumbnails: use_signal(HashMap::new),
        scroll: use_signal(|| 0.0),
        target: use_signal(|| 0.0),
        window: use_signal(Window::default),
        sidebar: use_signal(|| true),
        preview: use_signal(|| false),
        theme: use_signal(Theme::default),
        motion: use_signal(|| true),
        overlay: use_signal(|| Overlay::None),
        jobs: use_signal(Vec::new),
        notice: use_signal(|| None),
        keys: use_signal(String::new),
        editing_path: use_signal(|| false),
        focus: use_signal(|| 0),
        revision,
        serial: use_signal(|| 0),
        last_click: use_signal(|| None),
        press: use_signal(|| None),
        drop_target: use_signal(|| None),
    };
    use_context_provider(|| state);
    use_shell_events(state);
    use_scrolling(state);
    use_watcher(state);
    let mut palette = use_signal(|| theme::read_palette().unwrap_or_default());
    use_future(move || async move {
        theme::watch_palette(move |css| palette.set(css)).await;
    });
    // Blitz fetches font-face sources through linked stylesheets.
    let fonts = use_hook(theme::font_stylesheet);
    let variables = use_hook(viewport::css_variables);
    let loading = *listing.state().read() == UseResourceState::Pending;
    let error = listing
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err().cloned());
    let class = format!(
        "app {}{}{}",
        (state.theme)().class(),
        if (state.motion)() { "" } else { " no-motion" },
        if (state.sidebar)() { "" } else { " no-sidebar" },
    );
    rsx! {
        style { {include_str!("../../assets/style.css")} }
        style { {variables} }
        link { rel: "stylesheet", href: fonts }
        style { {palette()} }
        div { class,
            "data-capture-ready": "{!loading}",
            onkeydown: move |event| keyboard::handle(event, state),
            onclick: move |_| reclaim_focus(state),
            onmousedown: move |event| press_background(state, event),
            onmousemove: move |event| drag::moved(state, event),
            onmouseup: move |_| drag::released(state),
            FocusTarget { epoch: (state.focus)() }
            if (state.sidebar)() && !((state.view)() == View::Gallery && (state.gallery_focus)()) { sidebar::Sidebar {} }
            main { class: "main",
                topbar::Topbar {}
                if let Some(error) = error {
                    div { class: "pane",
                        div { class: "empty error", "Can\u{2019}t open this folder" }
                        div { class: "empty-detail", {error} }
                    }
                } else {
                    views::Content {}
                }
                overlay::Dock {}
            }
            if (state.preview)() && (state.view)() != View::Columns && !((state.view)() == View::Gallery && (state.gallery_focus)()) {
                aside { class: "side-preview",
                    if let Some(entry) = state.cursor_entry() {
                        preview::Preview { key: "{entry.uri()}", entry, compact: false }
                    }
                }
            }
            overlay::Overlays {}
        }
    }
}

/// Blitz clears focus on clicks outside inputs, and key events then miss the app.
fn reclaim_focus(state: Explorer) {
    let typing = (state.editing_path)()
        || (state.filtering)()
        || matches!(
            *state.overlay.peek(),
            Overlay::Console(_) | Overlay::SaveAs(_)
        );
    if !typing {
        state.refocus();
    }
}

fn press_background(mut state: Explorer, event: MouseEvent) {
    match event.trigger_button() {
        Some(MouseButton::Secondary) => {
            let point = event.client_coordinates();
            state.overlay.set(Overlay::Menu(state::Menu {
                x: point.x,
                y: point.y,
                target: None,
            }));
        }
        Some(MouseButton::Fourth) => state.history(false),
        Some(MouseButton::Fifth) => state.history(true),
        _ => {
            if matches!(*state.overlay.peek(), Overlay::Menu(_)) {
                state.overlay.set(Overlay::None);
            }
        }
    }
}

#[component]
fn FocusTarget(epoch: u64) -> Element {
    // Blitz computes focusability before Dioxus assigns tabindex, so this is a button.
    rsx! { for epoch in [epoch] { button { key: "{epoch}", class: "focus-target", autofocus: true, aria_label: "File navigation" } } }
}

fn use_shell_events(state: Explorer) {
    use_future(move || async move {
        let Some(mut events) = shell::take_events() else {
            return;
        };
        while let Some(event) = events.recv().await {
            handle_shell_event(state, event);
        }
    });
}

fn handle_shell_event(mut state: Explorer, event: ShellEvent) {
    match event {
        ShellEvent::Resize { width, height } => state.window.set(Window { width, height }),
        ShellEvent::Wheel { delta, x, y, zoom } => wheel(state, delta, x, y, zoom),
        ShellEvent::DragOver { x, y } => drag::over(state, x, y),
        ShellEvent::DragLeave => drag::leave(state),
        ShellEvent::Drop { x, y, paths } => drag::dropped(state, x, y, paths),
    }
}

fn wheel(mut state: Explorer, delta: f64, x: f64, y: f64, zoom: bool) {
    if delta == 0.0 {
        return;
    }
    if zoom {
        commands::run(
            state,
            keymap::Command::Zoom(if delta < 0.0 { 1 } else { -1 }),
        );
        return;
    }
    let layout = state.layout();
    if !layout.contains(x, y) || *state.overlay.peek() != Overlay::None {
        return;
    }
    if (state.view)() == View::Gallery && (state.gallery_focus)() {
        return;
    }
    let target = layout.clamp(*state.target.peek() + delta, state.entries.peek().len());
    if target != *state.target.peek() {
        state.target.set(target);
    }
    if target != *state.scroll.peek() {
        state.scroll.set(target);
    }
}

/// Eases the rendered scroll offset toward the target, one frame at a time.
fn use_scrolling(mut state: Explorer) {
    use_resource(move || {
        let target = (state.target)();
        async move {
            loop {
                let next = viewport::step(*state.scroll.peek(), target, *state.motion.peek());
                if next != *state.scroll.peek() {
                    state.scroll.set(next);
                }
                if next == target {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        }
    });
    // A remembered or `--select` cursor may start below the fold; show it once the
    // folder's listing arrives.
    let mut shown = use_signal(|| None::<PathBuf>);
    use_effect(move || {
        let current = state.navigation.read().current().to_path_buf();
        if !state.entries.read().is_empty() && shown.peek().as_ref() != Some(&current) {
            shown.set(Some(current));
            state.set_cursor(state.index());
        }
    });
    // Window, view, or filter changes can leave the offset past the end.
    use_effect(move || {
        let count = state.entries.read().len();
        let layout = state.layout();
        let clamped = layout.clamp(*state.target.peek(), count);
        if clamped != *state.target.peek() {
            state.target.set(clamped);
        }
    });
}

fn use_watcher(mut state: Explorer) {
    use_resource(move || {
        let path = state.navigation.read().current().to_path_buf();
        async move {
            crate::watch::folder(&path, move || state.revision += 1).await;
        }
    });
}
