use super::{
    commands, drag,
    keymap::Command,
    preview::Preview,
    state::{Explorer, Menu, Overlay},
    viewport::{Layout, PARENT_SHARE, PREVIEW_SHARE, View},
};
use crate::{
    filesystem::{self, Entry},
    icons::{FileArt, Glyph},
    thumbnails::{self, Flavor},
};
use dioxus_native::prelude::{dioxus_elements::input_data::MouseButton, *};
use std::{path::PathBuf, time::Duration};

const DOUBLE_CLICK: Duration = Duration::from_millis(400);

#[component]
pub fn Content() -> Element {
    let state = use_context::<Explorer>();
    let view = (state.view)();
    let path = state.current();
    let body = if view == View::Columns {
        rsx! {
            div { class: "miller",
                ParentColumn { key: "{path.display()}", path: path.clone() }
                div { class: "miller-current", Items {} }
                div { class: "miller-preview",
                    if let Some(entry) = state.cursor_entry() {
                        Preview { key: "{entry.uri()}", entry, compact: true }
                    }
                }
            }
        }
    } else {
        rsx! { Items {} }
    };
    rsx! {
        section { key: "{path.display()}", aria_label: "Folder contents",
            class: if state.drop_target.read().as_ref() == Some(&path) { "pane arrive dropping" } else { "pane arrive" },
            {body}
        }
    }
}

#[component]
fn Items() -> Element {
    let state = use_context::<Explorer>();
    let layout = state.layout();
    let view = (state.view)();
    let entries = state.entries.read();
    let count = entries.len();
    let scroll = (state.scroll)();
    let range = layout.visible(scroll, count);
    let offset = (range.start / layout.columns) as f64 * layout.row - scroll;
    let cursor = state.index();
    let marks = state.marks.read();
    let class = if view == View::Grid { "grid" } else { "rows" };
    let limit = name_limit(view, &layout);
    // Exact pixel tracks: Blitz collapses `1fr` tracks whose items use percentage widths.
    let style = format!(
        "top:{offset}px;grid-template-columns:repeat({}, {}px);--zoom:{};",
        layout.columns,
        (layout.width / layout.columns as f64).floor(),
        (state.zoom)()
    );
    rsx! {
        Thumbnails {}
        if count == 0 {
            Empty {}
        }
        div { class, style, "data-scroll": "{scroll}",
            for (index, entry) in entries[range.clone()].iter().enumerate().map(|(i, e)| (i + range.start, e)) {
                Item {
                    key: "{entry.uri()}",
                    entry: entry.clone(),
                    index,
                    view,
                    cursor: index == cursor,
                    marked: marks.contains(&entry.path),
                    cut: state.is_cut(&entry.path),
                    drop: state.drop_target.read().as_ref() == Some(&entry.path),
                    limit,
                }
            }
        }
        Scrollbar { layout, count, scroll }
    }
}

#[component]
fn Item(
    entry: Entry,
    index: usize,
    view: View,
    cursor: bool,
    marked: bool,
    cut: bool,
    drop: bool,
    limit: usize,
) -> Element {
    let state = use_context::<Explorer>();
    let class = format!(
        "item {}{}{}{}{}",
        if view == View::Grid { "card" } else { "row" },
        if cursor { " cursor" } else { "" },
        if marked { " marked" } else { "" },
        if cut { " cut" } else { "" },
        if drop { " drop" } else { "" },
    );
    let path = entry.path.clone();
    let pressed = entry.clone();
    let name = filesystem::shorten(&entry.name, limit);
    rsx! {
        div { class, title: "{entry.name}",
            onclick: move |event| click(state, index, path.clone(), event),
            onmousedown: move |event| press(state, index, pressed.clone(), event),
            if view == View::Grid {
                div { class: "art", FileArt { entry: entry.clone(), thumbnail: thumbnail(state, &entry) } }
                span { class: "name", {name.clone()} }
            } else {
                span { class: "glyph", Glyph { entry: entry.clone() } }
                span { class: "name", {name.clone()} }
                if view == View::List {
                    span { class: "meta size", "{entry.size_label()}" }
                    span { class: "meta date", "{entry.date_label()}" }
                } else if !entry.folder {
                    span { class: "meta size", "{entry.size_label()}" }
                }
            }
        }
    }
}

/// Characters that fit a label, from the average glyph width of the UI font.
fn name_limit(view: View, layout: &Layout) -> usize {
    const GLYPH: f64 = 7.0;
    let width = match view {
        View::Grid => return (layout.row / 8.2) as usize,
        View::List => layout.width - 260.0,
        View::Columns => layout.width - 130.0,
    };
    ((width / GLYPH) as usize).max(8)
}

fn thumbnail(state: Explorer, entry: &Entry) -> Option<String> {
    state.thumbnails.read().get(&entry.path).cloned()
}

fn click(mut state: Explorer, index: usize, path: PathBuf, event: MouseEvent) {
    let modifiers = event.modifiers();
    if modifiers.ctrl() {
        let mut marks = state.marks.write();
        if !marks.remove(&path) {
            marks.insert(path);
        }
    } else if modifiers.shift() {
        let from = state.index();
        let span = from.min(index)..=from.max(index);
        let paths: Vec<_> = state.entries.read()[span]
            .iter()
            .map(|e| e.path.clone())
            .collect();
        state.marks.write().extend(paths);
    } else {
        let now = std::time::Instant::now();
        let double = state
            .last_click
            .peek()
            .as_ref()
            .is_some_and(|(last, time)| *last == path && now - *time < DOUBLE_CLICK);
        state.last_click.set((!double).then_some((path, now)));
        state.marks.write().clear();
        state.set_cursor(index);
        if double {
            commands::run(state, Command::Open);
        }
        return;
    }
    state.set_cursor(index);
}

fn press(mut state: Explorer, index: usize, entry: Entry, event: MouseEvent) {
    match event.trigger_button() {
        Some(MouseButton::Secondary) => {
            event.stop_propagation();
            if !state.marks.read().contains(&entry.path) {
                state.marks.write().clear();
            }
            state.set_cursor(index);
            let point = event.client_coordinates();
            state.overlay.set(Overlay::Menu(Menu {
                x: point.x,
                y: point.y,
                target: Some(entry),
            }));
        }
        Some(MouseButton::Primary) if !event.modifiers().ctrl() && !event.modifiers().shift() => {
            let point = event.client_coordinates();
            drag::pressed(state, index, entry.path, point.x, point.y);
        }
        Some(MouseButton::Auxiliary) if entry.folder => {
            event.stop_propagation();
            commands::open_tab(state, entry.path);
        }
        _ => {}
    }
}

/// Requests Tumbler thumbnails for the visible grid plus one screen ahead,
/// after scrolling settles, and keeps every result for this folder.
#[component]
fn Thumbnails() -> Element {
    let mut state = use_context::<Explorer>();
    let batch = use_memo(move || {
        if (state.view)() != View::Grid {
            return Vec::new();
        }
        let entries = state.entries.read();
        let scroll = (state.scroll)();
        let range = state.layout().visible(scroll, entries.len());
        let ahead = (range.end + range.len()).min(entries.len());
        entries[range.start..ahead]
            .iter()
            .filter(|entry| thumbnails::can_preview(entry))
            .cloned()
            .collect::<Vec<_>>()
    });
    // Grid art is 128 px wide at zoom 1; a zoom change swaps the flavor.
    let flavor = use_memo(move || Flavor::covering(128.0 * (state.zoom)()));
    use_effect(move || {
        flavor();
        state.thumbnails.write().clear();
    });
    let loading = use_resource(move || {
        let batch = batch();
        let flavor = flavor();
        async move {
            if batch.is_empty() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(90)).await;
            thumbnails::load(batch, flavor, move |images| {
                state.thumbnails.write().extend(images)
            })
            .await
        }
    });
    let ready = *loading.state().read() != UseResourceState::Pending;
    rsx! { span { hidden: true, "data-capture-ready": "{ready}" } }
}

#[component]
fn Scrollbar(layout: Layout, count: usize, scroll: f64) -> Element {
    let max = layout.max_scroll(count);
    if max <= 0.0 {
        return rsx! {};
    }
    let total = max + layout.height;
    let size = (layout.height / total * layout.height).max(28.0);
    let top = scroll / max * (layout.height - size);
    rsx! { div { class: "scrollbar", style: "height:{size}px;top:{top}px;" } }
}

#[component]
fn Empty() -> Element {
    let state = use_context::<Explorer>();
    let text = if !state.query.read().is_empty() {
        "Nothing matches"
    } else if state.in_trash() {
        "Trash is empty"
    } else {
        "Empty folder"
    };
    rsx! { div { class: "empty", {text} } }
}

/// The ranger parent column: siblings of the current folder, centred on it.
#[component]
fn ParentColumn(path: PathBuf) -> Element {
    let state = use_context::<Explorer>();
    let parent = path.parent().map(PathBuf::from);
    let listing = use_resource(move || {
        let parent = parent.clone();
        let _ = (state.revision)();
        async move {
            let parent = parent?;
            tokio::task::spawn_blocking(move || filesystem::read_directory(&parent).ok())
                .await
                .ok()
                .flatten()
        }
    });
    let Some(Some(listing)) = listing.read().clone() else {
        return rsx! { div { class: "miller-parent" } };
    };
    let entries = filesystem::visible_entries(&listing, "", (state.hidden)(), (state.order)());
    let position = entries
        .iter()
        .position(|entry| entry.path == path)
        .unwrap_or(0);
    let layout = state.layout();
    let rows = (layout.height / layout.row).floor() as usize;
    let start = position
        .saturating_sub(rows / 2)
        .min(entries.len().saturating_sub(rows));
    let limit = ((layout.width * PARENT_SHARE / (1.0 - PARENT_SHARE - PREVIEW_SHARE) - 50.0) / 7.0)
        as usize;
    rsx! {
        div { class: "miller-parent",
            for entry in entries.into_iter().skip(start).take(rows) {
                div { key: "{entry.uri()}",
                    class: if entry.path == path { "item row cursor" } else { "item row" },
                    onclick: {
                        let target = entry.path.clone();
                        let child = path.clone();
                        move |_| if target != child { state.visit(target.clone()) }
                    },
                    span { class: "glyph", Glyph { entry: entry.clone() } }
                    span { class: "name", {filesystem::shorten(&entry.name, limit)} }
                }
            }
        }
    }
}
