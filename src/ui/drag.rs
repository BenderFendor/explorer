//! Drag and drop in the UI: starting drags from items, and dropping files from any app.

use super::{commands, state::Explorer};
use crate::{dnd, operations};
use dioxus_native::prelude::{dioxus_elements::input_data::MouseButton, *};
use std::path::PathBuf;

/// Pointer travel before a press becomes a drag, in logical pixels.
const THRESHOLD: f64 = 8.0;

/// A press on an item may become a drag. Pressing an unmarked item selects only it.
pub fn pressed(mut state: Explorer, index: usize, path: PathBuf, x: f64, y: f64) {
    if !state.marks.read().contains(&path) {
        state.marks.write().clear();
        state.set_cursor(index);
    }
    state.press.set(Some((x, y)));
}

pub fn released(mut state: Explorer) {
    state.press.set(None);
}

pub fn moved(mut state: Explorer, event: MouseEvent) {
    let Some((x, y)) = *state.press.peek() else {
        return;
    };
    let point = event.client_coordinates();
    if !event.held_buttons().contains(MouseButton::Primary) {
        state.press.set(None);
        return;
    }
    if (point.x - x).hypot(point.y - y) < THRESHOLD {
        return;
    }
    state.press.set(None);
    let paths = state.targets();
    if !paths.is_empty() && !dnd::start_drag(&paths) {
        state.notify("Dragging out needs a Wayland session", true);
    }
}

/// The folder under the pointer, or the open folder when the pointer is between items.
fn target_folder(state: Explorer, x: f64, y: f64) -> PathBuf {
    let entries = state.entries.peek();
    state
        .layout()
        .index_at(x, y, *state.scroll.peek(), entries.len())
        .and_then(|index| entries.get(index))
        .filter(|entry| entry.folder)
        .map_or_else(|| state.current(), |entry| entry.path.clone())
}

pub fn over(mut state: Explorer, x: f64, y: f64) {
    let folder = target_folder(state, x, y);
    if state.drop_target.peek().as_ref() != Some(&folder) {
        state.drop_target.set(Some(folder));
    }
}

pub fn leave(mut state: Explorer) {
    state.drop_target.set(None);
}

pub fn dropped(mut state: Explorer, x: f64, y: f64, paths: Vec<PathBuf>) {
    let folder = target_folder(state, x, y);
    state.drop_target.set(None);
    if let Some(mode) = operations::drop_mode(&paths, &folder) {
        commands::start_transfer(state, paths, folder, mode);
    }
}
