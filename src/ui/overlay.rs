use super::{
    commands, keyboard,
    keymap::{Command, HELP},
    state::{Explorer, Menu, Overlay},
    viewport::View,
};
use crate::filesystem::Sort;
use dioxus_native::prelude::*;
use std::time::Duration;

#[component]
pub fn Overlays() -> Element {
    let state = use_context::<Explorer>();
    let overlay = state.overlay.read().clone();
    rsx! {
        match overlay {
            Overlay::Confirm(paths) => rsx! { Confirm { label: commands::describe(&paths) } },
            Overlay::Properties(path) => rsx! { Properties { path } },
            Overlay::SearchResults(query,results) => rsx! { SearchResults { query, results } },
            Overlay::AppChooser(files) => rsx! { AppChooser { files } },
            Overlay::Menu(menu) => rsx! { ContextMenu { menu } },
            Overlay::Help => rsx! { Help {} },
            Overlay::None | Overlay::Console(_) | Overlay::SaveAs(_) => rsx! {},
        }
    }
}

/// Bottom-centred layer over the content: notices, transfers, and the console.
#[component]
pub fn Dock() -> Element {
    let state = use_context::<Explorer>();
    let overlay = state.overlay.read().clone();
    rsx! {
        div { class: "dock",
            Notice {}
            Jobs {}
            match overlay {
                Overlay::Console(text) => rsx! { Console { key: "{text}", initial: text } },
                Overlay::SaveAs(pending) => rsx! { SaveAs { pending } },
                _ => rsx! {},
            }
        }
    }
}

#[component]
fn Console(initial: String) -> Element {
    let mut state = use_context::<Explorer>();
    let mut text = use_signal(|| initial.clone());
    use_hook(crate::shell::request_caret_at_end);
    rsx! {
        div { class: "console rise",
            span { class: "console-mark", ":" }
            input { aria_label: "Command", autofocus: true, value: "{text}",
                oninput: move |event| text.set(event.value()),
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    event.stop_propagation();
                    match event.key() {
                        Key::Enter => keyboard::submit(state, &text()),
                        Key::Escape => {
                            state.overlay.set(Overlay::None);
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
fn SaveAs(pending: std::sync::Arc<crate::imports::Pending>) -> Element {
    let mut state = use_context::<Explorer>();
    let mut name = use_signal(|| pending.suggested.clone());
    use_hook(crate::shell::request_caret_at_end);
    rsx! {
        div { class: "console rise",
            span { "Save as" }
            input { aria_label: "Save download as", autofocus: true, value: "{name}",
                oninput: move |event| name.set(event.value()),
                onclick: move |event| event.stop_propagation(),
                onkeydown: move |event| {
                    event.stop_propagation();
                    match event.key() {
                        Key::Enter => commands::save_as(state, pending.clone(), name()),
                        Key::Escape => { state.overlay.set(Overlay::None); state.refocus(); }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[component]
fn AppChooser(files:Vec<std::path::PathBuf>)->Element {
    let mut state=use_context::<Explorer>();
    let apps=files.first().map(|p|crate::app_chooser::list(p)).unwrap_or_default();
    rsx! {
        div {class:"scrim fade",
            div {class:"dialog search-dialog pop",role:"dialog",
                div {class:"search-heading",
                    span {"Open With"}
                    span {class:"dialog-button",onclick:move |_|state.overlay.set(Overlay::None),"Close"}
                }
                div {class:"search-hits",
                    for app in apps {
                        div {class:"search-hit",key:"{app.id}",title:"{app.desktop.display()}",
                            onclick:{
                                let files=files.clone();
                                move |_|{
                                    let result=crate::app_chooser::launch(&app,&files);
                                    match result {Ok(())=>state.overlay.set(Overlay::None),Err(e)=>state.notify(e,true)}
                                }
                            },
                            "{app.name}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SearchResults(query:String,results:Vec<std::path::PathBuf>)->Element {
    let mut state=use_context::<Explorer>();
    rsx! {
        div { class:"scrim fade",
            div {class:"dialog search-dialog pop",role:"dialog",
                div {class:"search-heading",
                    span {"Search: {query} · {} results",results.len()}
                    span {class:"dialog-button",onclick:move |_|state.overlay.set(Overlay::None),"Close"}
                }
                div {class:"search-hits",
                    for result in results {
                        div {key:"{result.display()}",class:"search-hit",
                            title:"{result.display()}",
                            onclick:move |_| {
                                let path=result.clone();
                                state.overlay.set(Overlay::None);
                                if path.is_dir(){state.visit(path);}
                                else if let Some(parent)=path.parent(){state.visit_with(parent.to_path_buf(),Some(path));}
                                state.refocus();
                            },
                            "{result.display()}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Properties(path: std::path::PathBuf) -> Element {
    let mut state = use_context::<Explorer>();
    let filename = path
        .file_name()
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());
    let metadata = std::fs::symlink_metadata(&path).ok();
    let kind = if metadata.as_ref().is_some_and(|m| m.is_dir()) {
        "Folder".to_owned()
    } else if metadata
        .as_ref()
        .is_some_and(|m| m.file_type().is_symlink())
    {
        "Symbolic link".to_owned()
    } else {
        mime_guess::from_path(&path)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_owned()
    };
    let size = metadata
        .as_ref()
        .map(|m| format!("{} bytes", m.len()))
        .unwrap_or_else(|| "Unavailable".into());
    let permissions = metadata
        .as_ref()
        .map(|m| {
            use std::os::unix::fs::PermissionsExt;
            format!("{:o}", m.permissions().mode() & 0o7777)
        })
        .unwrap_or_else(|| "Unavailable".into());
    let modified = metadata
        .and_then(|m| m.modified().ok())
        .map(crate::filesystem::format_time)
        .unwrap_or_else(|| "Unavailable".into());
    let image_info = crate::image_metadata::load(&path);
    let tags_text = image_info.tags.join(", ");
    rsx! {
        div { class: "scrim fade",
            div { class: "dialog property-dialog pop", role: "dialog",
                p { class: "property-title", "{filename}" }
                div { class: "property-line", span { "Location" } span { "{path.display()}" } }
                div { class: "property-line", span { "Type" } span { "{kind}" } }
                div { class: "property-line", span { "Size" } span { "{size}" } }
                div { class: "property-line", span { "Permissions" } span { "{permissions}" } }
                div { class: "property-line", span { "Modified" } span { "{modified}" } }
                if !image_info.source.is_empty() {
                    div { class: "property-line", span { "Source" } span { "{image_info.source}" } }
                }
                if !image_info.tags.is_empty() {
                    div { class: "property-line", span { "Tags" } span { "{tags_text}" } }
                }
                div { class: "dialog-actions",
                    span { class: "dialog-button",
                        onclick: move |_| { state.overlay.set(Overlay::None); state.refocus(); },
                        "Close"
                    }
                }
            }
        }
    }
}

#[component]
fn Confirm(label: String) -> Element {
    let mut state = use_context::<Explorer>();
    rsx! {
        div { class: "scrim fade",
            div { class: "dialog pop", role: "alertdialog",
                p { "Delete {label} permanently?" }
                p { class: "muted", "This cannot be undone." }
                div { class: "dialog-actions",
                    span { class: "dialog-button", onclick: move |_| { state.overlay.set(Overlay::None); state.refocus(); }, "Cancel  n" }
                    span { class: "dialog-button danger",
                        onclick: move |_| {
                            if let Overlay::Confirm(paths) = state.overlay.peek().clone() {
                                commands::delete(state, paths);
                            }
                            state.overlay.set(Overlay::None);
                            state.refocus();
                        },
                        "Delete  y"
                    }
                }
            }
        }
    }
}

type Choice = (&'static str, &'static str, Command);

fn item_choices(folder: bool, in_trash: bool) -> Vec<Choice> {
    let mut choices: Vec<Choice> = vec![("Open", "l", Command::Open)];
    if folder {
        choices.push(("Open in new tab", "middle", Command::NewTab));
        choices.push(("Pin folder", "", Command::PinSelection));
    } else {
        choices.push(("Open with…", "", Command::ChooseApp));
        choices.push(("Edit caption", "", Command::Console("caption ")));
        choices.push(("Edit source", "", Command::Console("source ")));
        choices.push(("Edit tags", "", Command::Console("tags ")));
        choices.push(("Embed metadata as XMP", "", Command::XmpWrite));
        choices.push(("Import embedded XMP", "", Command::XmpRead));
    }
    choices.extend([
        ("Cut", "dd", Command::Cut),
        ("Copy", "yy", Command::Yank),
        ("Rename", "cw", Command::Rename),
    ]);
    if in_trash {
        choices.push(("Restore", "", Command::Restore));
    } else {
        choices.push(("Move to trash", "Del", Command::Trash));
    }
    choices.push(("Properties", "", Command::Properties));
    choices.push(("Delete permanently", "dD", Command::Delete));
    choices
}

fn background_choices(in_trash: bool) -> Vec<Choice> {
    let mut choices: Vec<Choice> = vec![
        ("New folder", "Ctrl+Shift+N", Command::Console("mkdir ")),
        ("New file", ":touch", Command::Console("touch ")),
        ("Pin this folder", ":pin", Command::PinCurrent),
        ("Installed extensions", ":scripts", Command::ListScripts),
        ("Index this folder", ":index", Command::IndexFolder),
        ("Undo", ":undo", Command::Undo),
        ("Redo", ":redo", Command::Redo),
        ("Search indexed files", ":find", Command::Console("find ")),
        ("Paste", "pp", Command::Paste),
        ("Terminal here", "S", Command::Terminal),
        ("Grid", "Ctrl+1", Command::SetView(View::Grid)),
        ("List", "Ctrl+2", Command::SetView(View::List)),
        ("Columns", "Ctrl+3", Command::SetView(View::Columns)),
        ("Gallery", "Ctrl+4", Command::SetView(View::Gallery)),
        ("Sort by name", "on", Command::SortBy(Sort::Name)),
        ("Sort by modified", "om", Command::SortBy(Sort::Modified)),
        ("Sort by size", "os", Command::SortBy(Sort::Size)),
        ("Hidden files", "zh", Command::ToggleHidden),
    ];
    if in_trash {
        choices.push(("Empty trash", "", Command::EmptyTrash));
    }
    choices
}

#[component]
fn ContextMenu(menu: Menu) -> Element {
    let mut state = use_context::<Explorer>();
    let in_trash = state.in_trash();
    let target = menu.target.clone();
    let choices = match &target {
        Some(entry) => item_choices(entry.folder, in_trash),
        None => background_choices(in_trash),
    };
    let window = (state.window)();
    let height = choices.len() as f64 * 29.0 + 14.0;
    let x = menu.x.min(window.width - 224.0).max(4.0);
    let y = menu.y.min(window.height - height - 8.0).max(4.0);
    rsx! {
        div { class: "menu pop", role: "menu", style: "left:{x}px;top:{y}px;",
            onmousedown: move |event| event.stop_propagation(),
            for (label, hint, command) in choices {
                div { key: "{label}", class: "menu-item", role: "menuitem",
                    onclick: {
                        let target = target.clone();
                        move |_| {
                            state.overlay.set(Overlay::None);
                            match (&command, &target) {
                                (Command::NewTab, Some(entry)) => commands::open_tab(state, entry.path.clone()),
                                _ => commands::run(state, command.clone()),
                            }
                        }
                    },
                    span { {label} }
                    span { class: "hint", {hint} }
                }
            }
        }
    }
}

#[component]
fn Help() -> Element {
    let mut state = use_context::<Explorer>();
    rsx! {
        div { class: "scrim fade", onclick: move |_| state.overlay.set(Overlay::None),
            div { class: "help pop",
                for (keys, action) in HELP {
                    div { class: "help-row", span { class: "help-keys", {*keys} } span { {*action} } }
                }
            }
        }
    }
}

#[component]
fn Jobs() -> Element {
    let state = use_context::<Explorer>();
    let mut tick = use_signal(|| 0_u64);
    let running = use_memo(move || !state.jobs.read().is_empty());
    use_resource(move || {
        let running = running();
        async move {
            // The resource restarts when transfers finish, which ends this loop.
            if !running {
                return;
            }
            loop {
                tokio::time::sleep(Duration::from_millis(150)).await;
                tick += 1;
            }
        }
    });
    let _ = tick();
    let jobs = state.jobs.read().clone();
    rsx! {
        div { class: "jobs",
            for job in jobs {
                div { key: "{job.id}", class: "job rise",
                    span { "{job.label}" }
                    span { class: "job-bar", span { class: "job-fill", style: "width:{job.progress.fraction() * 100.0}%;" } }
                    span { class: "hint", onclick: move |_| job.progress.cancel.store(true, std::sync::atomic::Ordering::Relaxed), "cancel" }
                }
            }
        }
    }
}

#[component]
fn Notice() -> Element {
    let state = use_context::<Explorer>();
    let Some(notice) = state.notice.read().clone() else {
        return rsx! {};
    };
    rsx! {
        div { key: "{notice.id}", class: if notice.error { "notice rise error" } else { "notice rise" }, role: "status",
            "{notice.text}"
        }
    }
}
