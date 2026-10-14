use super::{
    keymap::Command,
    state::{Clipboard, Explorer, Job, Overlay},
    viewport::{self, View},
};
use crate::{
    filesystem::{self, Navigation},
    operations::{self, Progress, Transfer},
};
use dioxus_native::prelude::{dioxus_core::spawn_forever, *};
use std::{path::PathBuf, sync::Arc};

pub fn run(state: Explorer, command: Command) {
    let _ = navigate(state, &command)
        || places(state, &command)
        || select(state, &command)
        || clipboard(state, &command)
        || files(state, &command)
        || trash(state, &command)
        || prompts(state, &command)
        || display(state, &command)
        || tabs(state, &command);
}

fn navigate(state: Explorer, command: &Command) -> bool {
    let count = state.entries.read().len();
    match command {
        Command::Move(delta) => {
            let stride = if (state.view)() == View::Gallery && (state.gallery_focus)() {
                1
            } else {
                state.layout().columns as isize
            };
            state.move_by(delta * stride);
        },
        Command::Step(delta) => step(state, *delta),
        Command::Top => state.set_cursor(0),
        Command::Bottom => state.set_cursor(count.saturating_sub(1)),
        Command::Page(fraction) => page(state, *fraction),
        Command::Parent => state.parent(),
        Command::Left => {
            if (state.view)() == View::Gallery { state.move_by(-1) } else { state.parent() }
        }
        Command::Right => {
            if (state.view)() == View::Gallery { state.move_by(1) } else { open(state) }
        }
        Command::Open => open(state),
        Command::Back => state.history(false),
        Command::Forward => state.history(true),
        _ => return false,
    }
    true
}

fn page(state: Explorer, fraction: f64) {
    let step = state.layout().page(fraction.abs()) as isize;
    state.move_by(if fraction < 0.0 { -step } else { step });
}

fn step(state: Explorer, delta: isize) {
    if matches!((state.view)(), View::Grid | View::Gallery) {
        state.move_by(delta);
    } else if delta < 0 {
        state.parent();
    } else {
        open(state);
    }
}

fn places(mut state: Explorer, command: &Command) -> bool {
    match command {
        Command::Home => state.visit(filesystem::home()),
        Command::Root => state.visit(PathBuf::from("/")),
        Command::Trashcan => state.visit(operations::trash_files()),
        Command::Cd(path) => state.visit(filesystem::resolve(path.as_ref(), &state.current())),
        Command::SetBookmark(key) => {
            let current = state.current();
            state.bookmarks.write().insert(*key, current);
            state.notify(format!("Bookmark {key} set"), false);
        }
        Command::Bookmark(key) => jump(state, *key),
        Command::EditPath => state.editing_path.set(true),
        Command::NewWindow => report(state, new_window(state.current())),
        Command::Terminal => report(state, operations::terminal(&state.current())),
        _ => return false,
    }
    true
}

fn jump(state: Explorer, key: char) {
    if key == '\'' {
        return state.history(false);
    }
    let path = state.bookmarks.read().get(&key).cloned();
    match path {
        Some(path) => state.visit(path),
        None => state.notify(format!("No bookmark {key}"), true),
    }
}

fn select(mut state: Explorer, command: &Command) -> bool {
    match command {
        Command::Mark => {
            toggle_mark(state);
            state.move_by(1);
        }
        Command::MarkMove(delta) => {
            if let Some(entry) = state.cursor_entry() {
                state.marks.write().insert(entry.path);
            }
            state.move_by(delta * state.layout().columns as isize);
            if let Some(entry) = state.cursor_entry() {
                state.marks.write().insert(entry.path);
            }
        }
        Command::InvertMarks => {
            let all: Vec<_> = state
                .entries
                .read()
                .iter()
                .map(|e| e.path.clone())
                .collect();
            let mut marks = state.marks.write();
            *marks = all
                .into_iter()
                .filter(|path| !marks.contains(path))
                .collect();
        }
        Command::ClearMarks => state.marks.write().clear(),
        Command::SelectAll => {
            let all = state
                .entries
                .read()
                .iter()
                .map(|e| e.path.clone())
                .collect();
            state.marks.set(all);
        }
        Command::Filter => state.filtering.set(true),
        Command::Escape => escape(state),
        _ => return false,
    }
    true
}

fn clipboard(mut state: Explorer, command: &Command) -> bool {
    let mode = match command {
        Command::Yank => Transfer::Copy,
        Command::Cut => Transfer::Move,
        Command::Paste => {
            paste(state);
            return true;
        }
        _ => return false,
    };
    let paths = state.targets();
    if paths.is_empty() {
        return true;
    }
    let verb = if mode == Transfer::Copy {
        "Copied"
    } else {
        "Cut"
    };
    state.notify(format!("{verb} {}", describe(&paths)), false);
    if crate::clipboard::enabled() {
        let shared = paths.clone();
        spawn_forever(async move {
            let result = tokio::task::spawn_blocking(move || crate::clipboard::copy(&shared, mode))
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            report(state, result);
        });
    }
    state.clipboard.set(Some(Clipboard { paths, mode }));
    state.marks.write().clear();
    true
}

fn files(mut state: Explorer, command: &Command) -> bool {
    let folder = state.current();
    match command {
        Command::Rename => {
            if let Some(entry) = state.cursor_entry() {
                state
                    .overlay
                    .set(Overlay::Console(format!("rename {}", entry.name)));
            }
        }
        Command::RenameTo(name) => rename(state, name.clone()),
        Command::Download(arguments) => {
            let (url, name) = arguments
                .split_once(char::is_whitespace)
                .unwrap_or((arguments, ""));
            download(
                state,
                url.to_owned(),
                (!name.trim().is_empty()).then(|| name.trim().to_owned()),
            );
        }
        Command::MakeFolder(name) | Command::MakeFile(name) => {
            let folder_kind = matches!(command, Command::MakeFolder(_));
            let result = operations::create(&folder, name, folder_kind);
            select_created(state, result);
        }
        Command::OpenWith(program) => report(
            state,
            operations::run_with(program, &folder, &state.targets()),
        ),
        _ => return false,
    }
    true
}

fn trash(state: Explorer, command: &Command) -> bool {
    match command {
        Command::Trash if state.in_trash() => confirm_delete(state),
        Command::Trash => {
            let paths = state.targets();
            let label = format!("Moved {} to trash", describe(&paths));
            background(state, label, move || operations::trash(&paths));
        }
        Command::Delete => confirm_delete(state),
        Command::Restore => {
            let paths = state.targets();
            let label = format!("Restored {}", describe(&paths));
            background(state, label, move || operations::restore(&paths));
        }
        Command::EmptyTrash => background(state, "Trash emptied".into(), operations::empty_trash),
        _ => return false,
    }
    true
}

fn prompts(mut state: Explorer, command: &Command) -> bool {
    match command {
        Command::Help => state.overlay.set(Overlay::Help),
        Command::Console(text) => state.overlay.set(Overlay::Console((*text).to_owned())),
        _ => return false,
    }
    state.set_cursor(state.index());
    true
}

fn display(mut state: Explorer, command: &Command) -> bool {
    match command {
        Command::ToggleHidden => state.hidden.toggle(),
        Command::SortBy(sort) => {
            state.order.write().sort = *sort;
            state.notify(format!("Sorted by {}", sort.label()), false);
            crate::prefs::save_order((state.order)());
        }
        Command::Reverse => {
            state.order.write().reverse ^= true;
            crate::prefs::save_order((state.order)());
        }
        Command::Refresh => state.revision += 1,
        Command::TogglePreview => {
            if (state.view)() == View::Gallery && (state.gallery_focus)() {
                state.gallery_info.toggle();
            } else {
                state.preview.toggle();
            }
        },
        Command::ToggleSidebar => state.sidebar.toggle(),
        Command::SetView(view) => {
            if (state.view)() != View::Gallery && *view == View::Gallery {
                state.gallery_previous.set((state.view)());
            }
            state.gallery_focus.set(false);
            state.gallery_zoom.set(1.0);
            state.view.set(*view);
        }
        Command::ToggleGallery => {
            let next = if (state.view)() == View::Gallery {
                (state.gallery_previous)()
            } else {
                state.gallery_previous.set((state.view)());
                View::Gallery
            };
            state.gallery_focus.set(false);
            state.gallery_zoom.set(1.0);
            state.view.set(next);
        }
        Command::Zoom(direction) => {
            if (state.view)() == View::Gallery && (state.gallery_focus)() {
                state.gallery_zoom.set(viewport::zoom_step((state.gallery_zoom)(), *direction));
            } else {
                state.zoom.set(viewport::zoom_step((state.zoom)(), *direction));
                if (state.view)() != View::Gallery {
                    state.view.set(View::Grid);
                }
            }
        }

        _ => return false,
    }
    let index = state.index();
    state.set_cursor(index);
    true
}

fn tabs(state: Explorer, command: &Command) -> bool {
    let count = state.tabs.read().len();
    let active = (state.tab)();
    match command {
        Command::NewTab => open_tab(state, state.current()),
        Command::NextTab => switch_tab(state, (active + 1) % count),
        Command::PreviousTab => switch_tab(state, (active + count - 1) % count),
        Command::CloseTab => close_tab(state),
        _ => return false,
    }
    true
}

pub fn open_tab(mut state: Explorer, path: PathBuf) {
    let active = (state.tab)();
    state.tabs.write().insert(active + 1, Navigation::new(path));
    switch_tab(state, active + 1);
}

pub fn switch_tab(mut state: Explorer, index: usize) {
    let active = (state.tab)();
    if index == active || index >= state.tabs.read().len() {
        return;
    }
    let current = state.navigation.read().clone();
    let target = state.tabs.read()[index].clone();
    state.tabs.write()[active] = current;
    state.navigation.set(target);
    state.tab.set(index);
    let select = state.remembered.read().get(&state.current()).cloned();
    state.arrive(select);
}

fn close_tab(mut state: Explorer) {
    let count = state.tabs.read().len();
    if count == 1 {
        if state.jobs.read().is_empty() {
            std::process::exit(0);
        }
        state.notify("Transfers are still running", true);
        return;
    }
    let active = (state.tab)();
    let next = if active + 1 < count {
        active + 1
    } else {
        active - 1
    };
    let target = state.tabs.read()[next].clone();
    state.tabs.write().remove(active);
    state.navigation.set(target);
    state.tab.set(next.min(count - 2));
    state.arrive(None);
}

fn open(mut state: Explorer) {
    let Some(entry) = state.cursor_entry() else {
        return;
    };
    if (state.view)() == View::Gallery {
        state.gallery_zoom.set(1.0);
        state.gallery_focus.set(true);
        return;
    }
    if entry.folder {
        state.visit(entry.path);
        return;
    }
    let marked = state.targets();
    let paths = if marked.contains(&entry.path) {
        marked
    } else {
        vec![entry.path]
    };
    for path in paths {
        report(state, operations::open(&path));
    }
}

fn toggle_mark(mut state: Explorer) {
    let Some(entry) = state.cursor_entry() else {
        return;
    };
    let mut marks = state.marks.write();
    if !marks.remove(&entry.path) {
        marks.insert(entry.path);
    }
}

fn escape(mut state: Explorer) {
    state.keys.set(String::new());
    if *state.overlay.peek() == Overlay::None && (state.view)() == View::Gallery && (state.gallery_focus)() {
        state.gallery_focus.set(false);
        state.gallery_zoom.set(1.0);
        state.refocus();
        return;
    }
    if *state.overlay.peek() != Overlay::None {
        state.overlay.set(Overlay::None);
    } else if state.filtering.peek().to_owned() || !state.query.peek().is_empty() {
        state.filtering.set(false);
        state.query.set(String::new());
    } else {
        state.marks.write().clear();
    }
    state.refocus();
}

fn confirm_delete(mut state: Explorer) {
    let paths = state.targets();
    if !paths.is_empty() {
        state.overlay.set(Overlay::Confirm(paths));
    }
}

pub fn delete(state: Explorer, paths: Vec<PathBuf>) {
    let label = format!("Deleted {}", describe(&paths));
    background(state, label, move || operations::delete(&paths));
}

fn rename(state: Explorer, name: String) {
    let Some(entry) = state.cursor_entry() else {
        return;
    };
    select_created(state, operations::rename(&entry.path, &name));
}

fn select_created(mut state: Explorer, result: Result<PathBuf, String>) {
    match result {
        Ok(path) => {
            if path.parent() != Some(state.current().as_path()) {
                state.notify(format!("Saved {}", path.display()), false);
                return;
            }
            state.cursor.write().path = Some(path);
            state.revision += 1;
        }
        Err(error) => state.notify(error, true),
    }
}

/// Pastes from the session clipboard, so files copied in other apps arrive too.
/// Without a session clipboard (tests, captures) the in-app clipboard is used.
fn paste(state: Explorer) {
    let folder = state.current();
    let internal = state
        .clipboard
        .read()
        .clone()
        .map(|clip| crate::clipboard::Payload::Files(clip.mode, clip.paths));
    if !crate::clipboard::enabled() {
        return finish_paste(state, folder, internal);
    }
    spawn_forever(async move {
        match tokio::task::spawn_blocking(crate::clipboard::paste).await {
            Ok(Ok(found)) => finish_paste(state, folder, found.or(internal)),
            Ok(Err(error)) => {
                state.notify(error, true);
                finish_paste(state, folder, internal);
            }
            Err(error) => state.notify(error.to_string(), true),
        }
    });
}

fn finish_paste(mut state: Explorer, folder: PathBuf, found: Option<crate::clipboard::Payload>) {
    let Some(found) = found else {
        state.notify("Nothing to paste", true);
        return;
    };
    match found {
        crate::clipboard::Payload::Files(mode, paths) => {
            if mode == Transfer::Move {
                state.clipboard.set(None);
            }
            start_transfer(state, paths, folder, mode);
        }
        crate::clipboard::Payload::Bytes { name, data } => {
            spawn_forever(async move {
                let result = tokio::task::spawn_blocking(move || {
                    crate::imports::bytes(&folder, &name, &data)
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
                select_created(state, result);
            });
        }
        crate::clipboard::Payload::Link(url) => download_into(state, folder, url, None),
    }
}

fn download(state: Explorer, url: String, name: Option<String>) {
    download_into(state, state.current(), url, name);
}

fn download_into(mut state: Explorer, folder: PathBuf, url: String, name: Option<String>) {
    if let Err(error) = crate::imports::url(&url).and_then(|_| {
        name.as_deref()
            .map(operations::valid_name)
            .transpose()
            .map(drop)
    }) {
        state.notify(error, true);
        return;
    }
    let progress = Arc::new(Progress::default());
    state.serial += 1;
    let id = (state.serial)();
    state.jobs.write().push(Job {
        id,
        label: "Downloading".into(),
        progress: progress.clone(),
    });
    spawn_forever(async move {
        let result = tokio::task::spawn_blocking(move || {
            crate::imports::download(&folder, &url, name.as_deref(), &progress)
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        state.jobs.write().retain(|job| job.id != id);
        match result {
            Ok(crate::imports::Download::Saved(path)) => select_created(state, Ok(path)),
            Ok(crate::imports::Download::Name(pending)) => {
                state.overlay.set(Overlay::SaveAs(Arc::new(pending)))
            }
            Err(error) => state.notify(error, true),
        }
    });
}

pub fn save_as(mut state: Explorer, pending: Arc<crate::imports::Pending>, name: String) {
    let result = pending.save(&name);
    if result.is_ok() {
        state.overlay.set(Overlay::None);
        state.refocus();
    }
    select_created(state, result);
}

/// Copies or moves `paths` into `folder` as a background job with progress.
pub fn start_transfer(mut state: Explorer, paths: Vec<PathBuf>, folder: PathBuf, mode: Transfer) {
    let verb = if mode == Transfer::Copy {
        "Copying"
    } else {
        "Moving"
    };
    let label = format!("{verb} {}", describe(&paths));
    let progress = Arc::new(Progress::default());
    state.serial += 1;
    let id = (state.serial)();
    state.jobs.write().push(Job {
        id,
        label,
        progress: progress.clone(),
    });
    spawn_forever(async move {
        let result = tokio::task::spawn_blocking(move || {
            operations::transfer(&paths, &folder, mode, &progress)
        })
        .await
        .unwrap_or_else(|error| Err(error.to_string()));
        state.jobs.write().retain(|job| job.id != id);
        match result {
            Ok(created) => {
                if let Some(first) = created.first() {
                    state.cursor.write().path = Some(first.clone());
                }
                state.revision += 1;
            }
            Err(error) => state.notify(error, true),
        }
    });
}

/// Runs a short blocking operation off the UI thread, then reports and refreshes.
fn background(
    mut state: Explorer,
    done: String,
    operation: impl FnOnce() -> Result<(), String> + Send + 'static,
) {
    spawn_forever(async move {
        let result = tokio::task::spawn_blocking(operation)
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
        state.marks.write().clear();
        state.revision += 1;
        match result {
            Ok(()) => state.notify(done, false),
            Err(error) => state.notify(error, true),
        }
    });
}

fn report(state: Explorer, result: Result<(), String>) {
    if let Err(error) = result {
        state.notify(error, true);
    }
}

fn new_window(folder: PathBuf) -> Result<(), String> {
    let program = std::env::current_exe().map_err(|error| error.to_string())?;
    std::process::Command::new(program)
        .arg("--")
        .arg(folder)
        .spawn()
        .map(drop)
        .map_err(|error| error.to_string())
}

pub fn describe(paths: &[PathBuf]) -> String {
    match paths {
        [one] => format!("\u{201c}{}\u{201d}", operations::display_name(one)),
        many => format!("{} items", many.len()),
    }
}
