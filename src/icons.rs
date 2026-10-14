use crate::{filesystem::Entry, thumbnails};
use dioxus_native::prelude::*;

const FOLDER: &str = "M3 6.5A1.5 1.5 0 0 1 4.5 5h4.2l2 2h8.8A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z";
const FILE: &str = "M6 3h8l4 4v14H6zM14 3v4h4";

/// Small line glyph for list rows.
#[component]
pub fn Glyph(entry: Entry) -> Element {
    let (path, class) = if entry.folder {
        (FOLDER, "glyph-folder")
    } else {
        (FILE, "glyph-file")
    };
    rsx! {
        svg { class, width: "16", height: "16", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.6", stroke_linejoin: "round", "aria-hidden": "true",
            path { d: path }
        }
    }
}

/// Large artwork for grid cards and the preview pane: a Tumbler thumbnail when one
/// exists, otherwise a folder or a labelled sheet.
#[component]
pub fn FileArt(entry: Entry, thumbnail: Option<String>) -> Element {
    if entry.folder {
        return rsx! { span { class: "folder-art", aria_hidden: "true",
            span { class: "folder-tab" } span { class: "folder-back" } span { class: "folder-front" }
        } };
    }
    if let Some(uri) = thumbnail {
        return rsx! { span { class: "thumb", img { src: uri, alt: "", draggable: "false" } } };
    }
    let label: String = if entry.extension.is_empty() {
        String::from("file")
    } else {
        entry.extension.chars().take(4).collect()
    };
    let pending = if thumbnails::can_preview(&entry) {
        " pending"
    } else {
        ""
    };
    rsx! { span { class: "sheet {entry.kind().to_lowercase()}{pending}", aria_hidden: "true",
        span { class: "sheet-label", {label} }
    } }
}
