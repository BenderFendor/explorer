use super::viewport::{Layout, View, Window};
use crate::{
    filesystem::{Entry, Navigation, Order},
    operations::{Progress, Transfer},
    theme::Theme,
};
use dioxus_native::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, PartialEq)]
pub enum Overlay {
    None,
    Console(String),
    SaveAs(Arc<crate::imports::Pending>),
    Confirm(Vec<PathBuf>),
    Menu(Menu),
    Help,
}

#[derive(Clone, PartialEq)]
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub target: Option<Entry>,
}

#[derive(Clone, PartialEq)]
pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    pub mode: Transfer,
}

#[derive(Clone)]
pub struct Job {
    pub id: u64,
    pub label: String,
    pub progress: Arc<Progress>,
}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Clone, PartialEq)]
pub struct Notice {
    pub id: u64,
    pub text: String,
    pub error: bool,
}

/// The cursor remembers its path so it survives refreshes, and its index so a
/// deleted item hands the cursor to its neighbour, as in ranger.
#[derive(Clone, Default, PartialEq)]
pub struct Cursor {
    pub path: Option<PathBuf>,
    pub index: usize,
}

impl Cursor {
    pub fn resolve(&self, entries: &[Entry]) -> usize {
        self.path
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| &entry.path == path))
            .unwrap_or(self.index)
            .min(entries.len().saturating_sub(1))
    }
}

#[derive(Clone, Copy)]
pub struct Explorer {
    pub navigation: Signal<Navigation>,
    pub tabs: Signal<Vec<Navigation>>,
    pub tab: Signal<usize>,
    pub entries: Memo<Vec<Entry>>,
    pub query: Signal<String>,
    pub filtering: Signal<bool>,
    pub hidden: Signal<bool>,
    pub order: Signal<Order>,
    pub view: Signal<View>,
    pub gallery_previous: Signal<View>,
    pub gallery_focus: Signal<bool>,
    pub gallery_zoom: Signal<f64>,
    pub gallery_info: Signal<bool>,
    pub zoom: Signal<f64>,
    pub cursor: Signal<Cursor>,
    pub marks: Signal<HashSet<PathBuf>>,
    pub clipboard: Signal<Option<Clipboard>>,
    pub remembered: Signal<HashMap<PathBuf, PathBuf>>,
    pub bookmarks: Signal<HashMap<char, PathBuf>>,
    pub thumbnails: Signal<HashMap<PathBuf, String>>,
    pub scroll: Signal<f64>,
    pub target: Signal<f64>,
    pub window: Signal<Window>,
    pub sidebar: Signal<bool>,
    pub preview: Signal<bool>,
    pub theme: Signal<Theme>,
    pub motion: Signal<bool>,
    pub overlay: Signal<Overlay>,
    pub jobs: Signal<Vec<Job>>,
    pub notice: Signal<Option<Notice>>,
    pub keys: Signal<String>,
    pub editing_path: Signal<bool>,
    pub focus: Signal<u64>,
    pub revision: Signal<u64>,
    pub serial: Signal<u64>,
    pub last_click: Signal<Option<(PathBuf, std::time::Instant)>>,
    /// Where a primary press started, until it becomes a drag or ends.
    pub press: Signal<Option<(f64, f64)>>,
    /// The folder that receives a drop in progress.
    pub drop_target: Signal<Option<PathBuf>>,
}

impl Explorer {
    pub fn current(&self) -> PathBuf {
        self.navigation.read().current().to_path_buf()
    }

    pub fn layout(&self) -> Layout {
        let focused = (self.view)() == View::Gallery && (self.gallery_focus)();
        Layout::new(
            (self.window)(),
            (self.view)(),
            (self.sidebar)() && !focused,
            (self.preview)() && !focused,
            (self.zoom)(),
        )
    }

    pub fn index(&self) -> usize {
        self.cursor.read().resolve(&self.entries.read())
    }

    pub fn cursor_entry(&self) -> Option<Entry> {
        self.entries.read().get(self.index()).cloned()
    }

    pub fn set_cursor(mut self, index: usize) {
        let entries = self.entries.read();
        let Some(entry) = entries.get(index) else {
            return;
        };
        self.cursor.set(Cursor {
            path: Some(entry.path.clone()),
            index,
        });
        let count = entries.len();
        drop(entries);
        let target = self.layout().follow(*self.target.peek(), index, count);
        self.target.set(target);
    }

    /// Moves the cursor; a move past either end stops at the first or last item.
    pub fn move_by(self, delta: isize) {
        let last = self.entries.read().len().saturating_sub(1);
        self.set_cursor(self.index().saturating_add_signed(delta).min(last));
    }

    /// Items an operation acts on: the marks in display order, else the cursor item.
    pub fn targets(&self) -> Vec<PathBuf> {
        let marks = self.marks.read();
        let entries = self.entries.read();
        if marks.is_empty() {
            return entries
                .get(self.index())
                .map(|entry| vec![entry.path.clone()])
                .unwrap_or_default();
        }
        entries
            .iter()
            .filter(|entry| marks.contains(&entry.path))
            .map(|entry| entry.path.clone())
            .collect()
    }

    pub fn visit(self, path: PathBuf) {
        self.visit_with(path, None);
    }

    /// Opens `path`, placing the cursor on `select` or on the item last used there.
    pub fn visit_with(mut self, path: PathBuf, select: Option<PathBuf>) {
        self.remember();
        let select = select.or_else(|| self.remembered.read().get(&path).cloned());
        self.navigation.write().visit(path);
        self.arrive(select);
    }

    pub fn history(mut self, forward: bool) {
        self.remember();
        if forward {
            self.navigation.write().forward();
        } else {
            self.navigation.write().back();
        }
        let select = self.remembered.read().get(&self.current()).cloned();
        self.arrive(select);
    }

    pub fn parent(self) {
        let current = self.current();
        if let Some(parent) = current.parent() {
            self.visit_with(parent.to_path_buf(), Some(current));
        }
    }

    fn remember(mut self) {
        if let Some(path) = self.cursor.read().path.clone() {
            let current = self.current();
            self.remembered.write().insert(current, path);
        }
    }

    pub fn arrive(mut self, select: Option<PathBuf>) {
        self.gallery_focus.set(false);
        self.gallery_zoom.set(1.0);
        self.cursor.set(Cursor {
            path: select,
            index: 0,
        });
        self.query.set(String::new());
        self.filtering.set(false);
        self.marks.write().clear();
        self.thumbnails.write().clear();
        self.scroll.set(0.0);
        self.target.set(0.0);
        self.overlay.set(Overlay::None);
        self.editing_path.set(false);
    }

    pub fn notify(mut self, text: impl Into<String>, error: bool) {
        self.serial += 1;
        let id = (self.serial)();
        self.notice.set(Some(Notice {
            id,
            text: text.into(),
            error,
        }));
        spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(if error { 6 } else { 3 })).await;
            if self
                .notice
                .peek()
                .as_ref()
                .is_some_and(|notice| notice.id == id)
            {
                self.notice.set(None);
            }
        });
    }

    /// Returns keyboard focus to navigation after clicks, overlays, or fields close.
    pub fn refocus(mut self) {
        self.focus += 1;
    }

    pub fn in_trash(&self) -> bool {
        self.current().starts_with(crate::operations::trash_files())
    }

    pub fn is_cut(&self, path: &Path) -> bool {
        self.clipboard.read().as_ref().is_some_and(|clipboard| {
            clipboard.mode == Transfer::Move && clipboard.paths.iter().any(|item| item == path)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::{read_directory, tests::Fixture};

    #[test]
    fn cursor_follows_path_then_falls_back_to_index() {
        let fixture = Fixture::new();
        for name in ["a", "b", "c"] {
            std::fs::write(fixture.0.join(name), "").unwrap();
        }
        let entries = read_directory(&fixture.0).unwrap().entries;
        let cursor = Cursor {
            path: Some(fixture.0.join("c")),
            index: 0,
        };
        assert_eq!(cursor.resolve(&entries), 2);
        let removed = Cursor {
            path: Some(fixture.0.join("gone")),
            index: 1,
        };
        assert_eq!(removed.resolve(&entries), 1);
        assert_eq!(removed.resolve(&entries[..1]), 0);
        assert_eq!(Cursor::default().resolve(&[]), 0);
    }
}
