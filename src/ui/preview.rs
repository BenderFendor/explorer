use super::state::Explorer;
use crate::{
    filesystem::{self, Entry},
    icons::{FileArt, Glyph},
    preview,
    thumbnails::{self, Flavor},
};
use dioxus_native::prelude::*;
use std::time::Duration;

#[derive(Clone, PartialEq)]
enum Content {
    Folder(Vec<Entry>),
    Image(String),
    Text(String),
    Nothing,
}

/// ranger-style preview: folder contents, an x-large Tumbler thumbnail, or text.
/// Keyed by entry, so moving the cursor cancels the previous load.
#[component]
pub fn Preview(entry: Entry, compact: bool) -> Element {
    let state = use_context::<Explorer>();
    let source = entry.clone();
    let content = use_resource(move || {
        let entry = source.clone();
        let hidden = (state.hidden)();
        let order = (state.order)();
        async move {
            // Skip work while the cursor is still moving.
            tokio::time::sleep(Duration::from_millis(70)).await;
            load(entry, hidden, order).await
        }
    });
    let content = content.read().clone();
    rsx! {
        div { class: if compact { "preview compact" } else { "preview" },
            match content {
                Some(Content::Folder(children)) => rsx! {
                    div { class: "preview-list arrive",
                        for child in children.into_iter().take(60) {
                            div { key: "{child.uri()}", class: "item row",
                                span { class: "glyph", Glyph { entry: child.clone() } }
                                span { class: "name", "{child.name}" }
                            }
                        }
                    }
                },
                Some(Content::Image(uri)) => rsx! {
                    div { class: "preview-art arrive", FileArt { entry: entry.clone(), thumbnail: uri } }
                },
                Some(Content::Text(text)) => rsx! { pre { class: "preview-text arrive", {text} } },
                _ => rsx! {
                    div { class: "preview-art", FileArt { entry: entry.clone(), thumbnail: None } }
                },
            }
            if !compact || !entry.folder {
                Facts { entry: entry.clone() }
            }
        }
    }
}

#[component]
fn Facts(entry: Entry) -> Element {
    let size = entry.size_label();
    rsx! {
        div { class: "facts",
            div { class: "facts-name", "{entry.name}" }
            div { class: "facts-line",
                span { "{entry.kind()}" }
                if !size.is_empty() { span { {size} } }
            }
            div { class: "facts-line", span { "{entry.date_label()}" } span { class: "mono", "{entry.permissions()}" } }
        }
    }
}

async fn load(entry: Entry, hidden: bool, order: filesystem::Order) -> Content {
    if entry.folder {
        let path = entry.path.clone();
        let listing = tokio::task::spawn_blocking(move || filesystem::read_directory(&path)).await;
        return match listing {
            Ok(Ok(listing)) => {
                Content::Folder(filesystem::visible_entries(&listing, "", hidden, order))
            }
            _ => Content::Nothing,
        };
    }
    if thumbnails::can_preview(&entry) {
        let mut image = None;
        let _ = thumbnails::load(vec![entry.clone()], Flavor::XLarge, |images| {
            image = images.into_values().next().or(image.take());
        })
        .await;
        return image.map_or(Content::Nothing, Content::Image);
    }
    let path = entry.path.clone();
    tokio::task::spawn_blocking(move || preview::text(&path))
        .await
        .ok()
        .flatten()
        .map_or(Content::Nothing, Content::Text)
}
