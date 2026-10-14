use super::{commands, state::Explorer};
use crate::filesystem;
use dioxus_native::prelude::*;
use std::path::PathBuf;

#[component]
pub fn Topbar() -> Element {
    let state = use_context::<Explorer>();
    rsx! {
        header { class: "topbar",
            if (state.editing_path)() {
                PathField {}
            } else {
                Breadcrumb {}
            }
            if (state.filtering)() || !state.query.read().is_empty() {
                FilterField {}
            }
            Status {}
            if (state.view)() == super::viewport::View::Gallery {
                span { class: "gallery-mode-select",
                    title: "Switch between grid, masonry and freeform board",
                    onclick: move |_| {
                        let next = match (state.gallery_mode)() {
                            super::state::GalleryMode::Grid => "masonry",
                            super::state::GalleryMode::Masonry => "board",
                            super::state::GalleryMode::Board => "grid",
                        };
                        commands::run(state, super::keymap::Command::GalleryMode(next.into()));
                    },
                    "{(state.gallery_mode)().label()}"
                }
            }
            Tabs {}
        }
    }
}

/// Path segments; the home folder collapses to `~` like a shell prompt.
fn segments(path: &std::path::Path) -> Vec<(String, PathBuf)> {
    let home = filesystem::home();
    let (mut built, rest) = match path.strip_prefix(&home) {
        Ok(rest) => (vec![(String::from("~"), home.clone())], rest.to_path_buf()),
        Err(_) => (
            vec![(String::from("/"), PathBuf::from("/"))],
            path.strip_prefix("/").unwrap_or(path).to_path_buf(),
        ),
    };
    let mut cursor = built[0].1.clone();
    for part in rest.iter() {
        cursor.push(part);
        built.push((part.to_string_lossy().into_owned(), cursor.clone()));
    }
    built
}

#[component]
fn Breadcrumb() -> Element {
    let mut state = use_context::<Explorer>();
    let parts = segments(&state.current());
    let last = parts.len() - 1;
    let rooted = parts[0].0 == "/";
    rsx! {
        nav { class: "crumbs", aria_label: "Folder path",
            for (index, (name, path)) in parts.into_iter().enumerate() {
                span { key: "{path.display()}",
                    class: "crumb-part",
                    if index > 1 || (index == 1 && !rooted) { span { class: "crumb-sep", "/" } }
                    span {
                    class: if index == last { "crumb current" } else { "crumb" },
                    title: if index == last { "Edit path (Ctrl+L)" } else { "" },
                    onclick: move |_| {
                        if index == last {
                            state.editing_path.set(true);
                        } else {
                            state.visit(path.clone());
                        }
                    },
                    {name}
                    }
                }
            }
        }
    }
}

#[component]
fn PathField() -> Element {
    let mut state = use_context::<Explorer>();
    let mut text = use_signal(|| state.current().display().to_string());
    use_hook(crate::shell::request_caret_at_end);
    rsx! {
        input { class: "path-field", aria_label: "Folder path", autofocus: true,
            value: "{text}",
            oninput: move |event| text.set(event.value()),
            onclick: move |event| event.stop_propagation(),
            onkeydown: move |event| {
                event.stop_propagation();
                match event.key() {
                    Key::Enter => {
                        let path = filesystem::resolve(text().trim().as_ref(), &state.current());
                        state.editing_path.set(false);
                        state.refocus();
                        state.visit(path);
                    }
                    Key::Escape => {
                        state.editing_path.set(false);
                        state.refocus();
                    }
                    _ => {}
                }
            }
        }
    }
}

#[component]
fn FilterField() -> Element {
    let mut state = use_context::<Explorer>();
    let focused = (state.filtering)();
    rsx! {
        div { class: "filter arrive",
            span { class: "filter-mark", "/" }
            input { key: "{focused}", aria_label: "Filter this folder", autofocus: focused,
                value: "{state.query}",
                oninput: move |event| {
                    state.query.set(event.value());
                    state.set_cursor(0);
                },
                onclick: move |event| { event.stop_propagation(); state.filtering.set(true); },
                onkeydown: move |event| {
                    event.stop_propagation();
                    match event.key() {
                        Key::Escape => {
                            state.query.set(String::new());
                            state.filtering.set(false);
                            state.refocus();
                        }
                        Key::Enter | Key::ArrowDown => {
                            state.filtering.set(false);
                            state.refocus();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[component]
fn Status() -> Element {
    let state = use_context::<Explorer>();
    let count = state.entries.read().len();
    let marked = state.marks.read().len();
    let keys = state.keys.read().clone();
    let order = (state.order)();
    let default_order = filesystem::Order {
        sort: filesystem::Sort::Modified,
        reverse: false,
    };
    let sort = if order == default_order {
        String::new()
    } else {
        format!(
            "{}{}",
            order.sort.label(),
            if order.reverse { " ↑" } else { "" }
        )
    };
    rsx! {
        div { class: "status",
            if !keys.is_empty() { span { class: "keys", {keys} } }
            if marked > 0 { span { class: "accent", "{marked} marked" } }
            if !sort.is_empty() { span { {sort} } }
            if (state.hidden)() { span { "hidden shown" } }
            span { if (state.view)() == super::viewport::View::Gallery { "{count} images" } else { "{count} items" } }
        }
    }
}

#[component]
fn Tabs() -> Element {
    let state = use_context::<Explorer>();
    let tabs = state.tabs.read();
    if tabs.len() < 2 {
        return rsx! {};
    }
    let active = (state.tab)();
    let current = state.current();
    rsx! {
        div { class: "tabs",
            for index in 0..tabs.len() {
                span { key: "{index}",
                    class: if index == active { "tab active" } else { "tab" },
                    onclick: move |_| commands::switch_tab(state, index),
                    {
                        let path = if index == active { current.clone() } else { tabs[index].current().to_path_buf() };
                        path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into())
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumbs_collapse_home_and_keep_root() {
        let home = filesystem::home();
        let names: Vec<_> = segments(&home.join("Pictures/edits"))
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["~", "Pictures", "edits"]);
        let root = segments(std::path::Path::new("/mnt/Big storage"));
        assert_eq!(root[0].0, "/");
        assert_eq!(
            root[2],
            ("Big storage".into(), PathBuf::from("/mnt/Big storage"))
        );
        assert_eq!(segments(std::path::Path::new("/")).len(), 1);
    }
}
