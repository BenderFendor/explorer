use super::{
    commands,
    keymap::{self, Resolution},
    state::{Explorer, Overlay},
};
use dioxus_native::prelude::*;

pub fn handle(event: KeyboardEvent, mut state: Explorer) {
    let key = event.key();
    let modifiers = event.modifiers();
    if overlay_key(&event, &key, state) {
        return;
    }
    if let Some(command) = keymap::chord(&key, modifiers) {
        event.prevent_default();
        state.keys.set(String::new());
        commands::run(state, command);
        return;
    }
    let Key::Character(text) = key else {
        return;
    };
    if modifiers.ctrl() || modifiers.alt() {
        return;
    }
    event.prevent_default();
    let sequence = format!("{}{text}", state.keys.peek());
    match keymap::resolve(&sequence) {
        Resolution::Run(command) => {
            state.keys.set(String::new());
            commands::run(state, command);
        }
        Resolution::Pending => state.keys.set(sequence),
        Resolution::Unknown => state.keys.set(String::new()),
    }
}

/// Confirm prompts take y/Enter; help and menus close on any key. Returns true when consumed.
fn overlay_key(event: &KeyboardEvent, key: &Key, mut state: Explorer) -> bool {
    let overlay = state.overlay.peek().clone();
    match overlay {
        Overlay::Confirm(paths) => {
            event.prevent_default();
            let yes = matches!(key, Key::Character(text) if text.eq_ignore_ascii_case("y"));
            if yes || *key == Key::Enter {
                commands::delete(state, paths);
            }
            state.overlay.set(Overlay::None);
            true
        }
        Overlay::Help | Overlay::Menu(_) => {
            state.overlay.set(Overlay::None);
            *key == Key::Escape
        }
        Overlay::Properties(_) => {
            if *key == Key::Escape {
                state.overlay.set(Overlay::None);
                state.refocus();
            }
            true
        }
        Overlay::None | Overlay::Console(_) | Overlay::SaveAs(_) => false,
    }
}

/// Runs a console line; errors stay in the console so they can be corrected.
pub fn submit(mut state: Explorer, line: &str) {
    match keymap::parse_console(line) {
        Ok(command) => {
            state.overlay.set(Overlay::None);
            state.refocus();
            commands::run(state, command);
        }
        Err(error) => state.notify(error, true),
    }
}
