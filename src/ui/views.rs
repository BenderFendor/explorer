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
    let body = if view == View::Gallery && (state.gallery_focus)() {
        rsx! { GalleryFocus {} }
    } else if view == View::Columns {
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
    if view == View::Gallery && (state.gallery_mode)() != super::state::GalleryMode::Grid {
        return rsx! { MasonryItems {} };
    }
    let entries = state.entries.read();
    let count = entries.len();
    let scroll = (state.scroll)();
    let range = layout.visible(scroll, count);
    let offset = (range.start / layout.columns) as f64 * layout.row - scroll;
    let cursor = state.index();
    let marks = state.marks.read();
    let class = match view {
        View::Grid => "grid",
        View::Gallery => "grid gallery-grid",
        _ => "rows",
    };
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


/// Aspect-aware masonry and freeform board. Only visible card rectangles are mounted.
#[component]
fn MasonryItems() -> Element {
    let state = use_context::<Explorer>();
    let geometry = state.gallery_layout.read();
    let entries = state.entries.read();
    let scroll = (state.scroll)();
    let indices = geometry.visible(scroll, state.layout().height);
    let current = state.index();
    let marks = state.marks.read();
    let height = geometry.height;
    let style = format!("height:{height}px;top:-{scroll}px;position:relative;--zoom:{};", (state.zoom)());
    rsx! {
        Thumbnails {}
        if entries.is_empty() { Empty {} }
        div { class: "gallery-masonry", style,
            for index in indices {
                if let (Some(entry),Some(rect)) = (entries.get(index),geometry.rects.get(index)) {
                    div { key: "{entry.uri()}", class: "gallery-masonry-slot",
                        style: "left:{}px;top:{}px;width:{}px;height:{}px;", rect.x,rect.y,rect.w,rect.h,
                        Item {
                            entry: entry.clone(), index, view: View::Gallery,
                            cursor: index==current, marked: marks.contains(&entry.path),
                            cut: state.is_cut(&entry.path), drop: false, limit: 0,
                        }
                    }
                }
            }
        }
        if height > state.layout().height {
            div { class: "scrollbar",
                style: "height:{}px;top:{}px;",
                    (state.layout().height / height * state.layout().height).max(28.0),
                    scroll / geometry.max_scroll(state.layout().height) *
                      (state.layout().height - (state.layout().height / height * state.layout().height).max(28.0))
            }
        }
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
        match view {
            View::Grid => "card",
            View::Gallery => "gallery-card",
            _ => "row",
        },
        if cursor { " cursor" } else { "" },
        if marked { " marked" } else { "" },
        if cut { " cut" } else { "" },
        if drop { " drop" } else { "" },
    );
    let path = entry.path.clone();
    let pressed = entry.clone();
    let name = filesystem::shorten(&entry.name, limit);
    let caption = if view == View::Gallery {
        crate::image_metadata::load(&entry.path).caption
    } else {
        String::new()
    };
    rsx! {
        div { class, title: "{entry.name}",
            onclick: move |event| click(state, index, path.clone(), event),
            onmousedown: move |event| press(state, index, pressed.clone(), event),
            if matches!(view, View::Grid | View::Gallery) {
                div { class: "art", FileArt { entry: entry.clone(), thumbnail: thumbnail(state, &entry) } }
                if view == View::Grid { span { class: "name", {name.clone()} } }
                if view == View::Gallery && !caption.is_empty() {
                    span { class: "gallery-caption", "{caption}" }
                }
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
        View::Gallery => return 0,
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
        let view = (state.view)();
        if !matches!(view, View::Grid | View::Gallery) {
            return Vec::new();
        }
        let entries = state.entries.read();
        if view == View::Gallery && !(state.gallery_focus)()
            && (state.gallery_mode)() != super::state::GalleryMode::Grid {
            let indices = state.gallery_layout.read().visible((state.scroll)(),state.layout().height*1.6);
            return indices.into_iter().filter_map(|i|entries.get(i)).filter(|entry| thumbnails::can_preview(entry)).cloned().collect::<Vec<_>>();
        }
        let range = if view == View::Gallery && (state.gallery_focus)() {
            let index = state.index();
            index.saturating_sub(5)..(index + 6).min(entries.len())
        } else {
            let scroll = (state.scroll)();
            state.layout().visible(scroll, entries.len())
        };
        let ahead = if view == View::Gallery && (state.gallery_focus)() {
            range.end
        } else {
            (range.end + range.len()).min(entries.len())
        };
        entries[range.start..ahead]
            .iter()
            .filter(|entry| thumbnails::can_preview(entry))
            .cloned()
            .collect::<Vec<_>>()
    });
    // Grid art is 128 px wide at zoom 1; a zoom change swaps the flavor.
    let flavor = use_memo(move || {
        let size = if (state.view)() == View::Gallery {
            212.0
        } else {
            128.0
        };
        Flavor::covering(size * (state.zoom)())
    });
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
    } else if (state.view)() == View::Gallery {
        "No images in this folder"
    } else if state.in_trash() {
        "Trash is empty"
    } else {
        "Empty folder"
    };
    rsx! { div { class: "empty", {text} } }
}

/// Large in-app image viewer with a window-bounded thumbnail strip.
#[component]
fn GalleryFocus() -> Element {
    let mut state = use_context::<Explorer>();
    let entries = state.entries.read();
    let index = state.index();
    let Some(entry) = entries.get(index).cloned() else {
        return rsx! { Empty {} };
    };
    let count = entries.len();
    let slots = ((state.layout().width / 88.0).floor() as usize).clamp(3, 9);
    let start = index
        .saturating_sub(slots / 2)
        .min(count.saturating_sub(slots));
    let strip: Vec<_> = entries
        .iter()
        .enumerate()
        .skip(start)
        .take(slots)
        .map(|(i, entry)| (i, entry.clone()))
        .collect();
    let source = entry.uri();
    let scale = (state.gallery_zoom)();
    let name = entry.name.clone();
    let size = entry.size_label();
    let date = entry.date_label();
    let metadata = crate::image_metadata::load(&entry.path);
    let tags_text = metadata.tags.join(", ");
    rsx! {
        div { class: "gallery-focus",
            Thumbnails {}
            div { class: "gallery-stage",
                img {
                    key: "{source}", class: "gallery-full",
                    src: source, alt: "{name}", draggable: "false",
                    style: "transform:scale({scale});",
                }
            }
            div { class: "gallery-filmstrip",
                for (position, image) in strip {
                    div {
                        key: "{image.uri()}",
                        class: if position == index { "gallery-film active" } else { "gallery-film" },
                        title: "{image.name}",
                        onclick: move |_| state.set_cursor(position),
                        if let Some(uri) = thumbnail(state, &image) {
                            img { src: uri, alt: "{image.name}", draggable: "false" }
                        } else {
                            span { class: "gallery-film-placeholder", "image" }
                        }
                    }
                }
            }
            if (state.gallery_info)() {
                div { class: "gallery-annotations",
                    span { class: "gallery-annotation-label",
                        onclick: move |_| state.overlay.set(Overlay::Console(String::from("caption "))),
                        if metadata.caption.is_empty() { "Add caption…" } else { "{metadata.caption}" }
                    }
                    span { class: "gallery-annotation-label",
                        onclick: move |_| state.overlay.set(Overlay::Console(String::from("source "))),
                        if metadata.source.is_empty() { "Add source…" } else { "{metadata.source}" }
                    }
                    span { class: "gallery-annotation-label",
                        onclick: move |_| state.overlay.set(Overlay::Console(String::from("tags "))),
                        if metadata.tags.is_empty() { "Add tags…" } else { "{tags_text}" }
                    }
                    span { class: "gallery-annotation-label", title: "Edit notes",
                        onclick: move |_| state.overlay.set(Overlay::Console(String::from("note "))),
                        if metadata.notes.is_empty() { "Add note…" } else { "Note: {metadata.notes}" }
                    }
                }
                div { class: "gallery-facts",
                    span { class: "gallery-filename", {name.clone()} }
                    span { class: "gallery-details", "{size}  ·  {date}  ·  {index + 1}/{count}  ·  {scale}×" }
                }
            }
        }
    }
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
