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
    } else {
        choices.push(("Open with…", "r", Command::Console("open_with ")));
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
    choices.push(("Delete permanently", "dD", Command::Delete));
    choices
}

fn background_choices(in_trash: bool) -> Vec<Choice> {
    let mut choices: Vec<Choice> = vec![
        ("New folder", "Ctrl+Shift+N", Command::Console("mkdir ")),
        ("New file", ":touch", Command::Console("touch ")),
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
