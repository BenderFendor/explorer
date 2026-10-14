use std::{
    cmp::Ordering,
    collections::HashSet,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub folder: bool,
    pub symlink: bool,
    pub hidden: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub extension: String,
    pub mode: u32,
    sort_name: String,
}

impl Entry {
    pub fn kind(&self) -> &'static str {
        if self.folder {
            return "Folder";
        }
        match self.extension.as_str() {
            "pdf" | "epub" | "djvu" => "Document",
            "zip" | "tar" | "gz" | "xz" | "zst" | "bz2" | "7z" | "rar" => "Archive",
            "rs" | "py" | "js" | "ts" | "tsx" | "c" | "h" | "cpp" | "go" | "lua" | "sh"
            | "html" | "css" | "json" | "toml" | "yaml" | "yml" => "Code",
            _ => match self.mime().type_().as_str() {
                "image" => "Image",
                "video" => "Video",
                "audio" => "Audio",
                "text" => "Text",
                "font" => "Font",
                _ if self.symlink => "Link",
                _ => "File",
            },
        }
    }

    pub fn mime(&self) -> mime_guess::Mime {
        mime_guess::from_path(&self.path).first_or_octet_stream()
    }

    pub fn uri(&self) -> String {
        url::Url::from_file_path(&self.path)
            .map(|url| url.to_string())
            .unwrap_or_default()
    }

    pub fn size_label(&self) -> String {
        if self.folder {
            String::new()
        } else {
            format_size(self.size)
        }
    }

    pub fn date_label(&self) -> String {
        self.modified.map(format_time).unwrap_or_default()
    }

    pub fn permissions(&self) -> String {
        let kind = if self.symlink {
            'l'
        } else if self.folder {
            'd'
        } else {
            '-'
        };
        let bits = (0..9).rev().map(|bit| {
            if self.mode & (1 << bit) == 0 {
                '-'
            } else {
                ['x', 'w', 'r'][bit % 3]
            }
        });
        std::iter::once(kind).chain(bits).collect()
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct Listing {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    pub warnings: usize,
}

pub fn read_directory(path: &Path) -> Result<Listing, String> {
    let directory = fs::read_dir(path).map_err(|error| error.to_string())?;
    // Thunar and Nautilus hide names listed in a folder's `.hidden` file.
    let listed: HashSet<String> = fs::read_to_string(path.join(".hidden"))
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    let mut listing = Listing {
        path: path.to_path_buf(),
        ..Listing::default()
    };
    for item in directory {
        match item.and_then(|item| read_entry(&item, &listed)) {
            Ok(entry) => listing.entries.push(entry),
            Err(_) => listing.warnings += 1,
        }
    }
    listing.entries.sort_by(|a, b| compare(a, b, Sort::Name));
    Ok(listing)
}

fn read_entry(item: &fs::DirEntry, listed: &HashSet<String>) -> std::io::Result<Entry> {
    let path = item.path();
    let symlink = item.file_type()?.is_symlink();
    // Links report their target's metadata; broken links fall back to the link itself.
    let metadata = fs::metadata(&path).or_else(|_| fs::symlink_metadata(&path))?;
    let name = item.file_name().to_string_lossy().into_owned();
    Ok(Entry {
        hidden: name.starts_with('.') || listed.contains(&name),
        sort_name: name.to_lowercase(),
        extension: path
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
        folder: metadata.is_dir(),
        size: metadata.len(),
        modified: metadata.modified().ok(),
        mode: metadata.permissions().mode(),
        name,
        path,
        symlink,
    })
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Sort {
    #[default]
    Name,
    Modified,
    Size,
    Type,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Modified => "modified",
            Self::Size => "size",
            Self::Type => "type",
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Order {
    pub sort: Sort,
    pub reverse: bool,
}

fn compare(a: &Entry, b: &Entry, sort: Sort) -> Ordering {
    match sort {
        Sort::Name => Ordering::Equal,
        Sort::Modified => b.modified.cmp(&a.modified),
        Sort::Size => b.size.cmp(&a.size),
        Sort::Type => a.extension.cmp(&b.extension),
    }
    .then_with(|| natural(&a.sort_name, &b.sort_name))
    .then_with(|| a.path.cmp(&b.path))
}

/// Compares digit runs by value, so "file2" sorts before "file10".
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    loop {
        match (a.first(), b.first()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (left, rest_a) = split_digits(a);
                let (right, rest_b) = split_digits(b);
                let (left, right) = (trim_zeros(left), trim_zeros(right));
                let order = left.len().cmp(&right.len()).then_with(|| left.cmp(right));
                if order != Ordering::Equal {
                    return order;
                }
                (a, b) = (rest_a, rest_b);
            }
            (Some(x), Some(y)) if x != y => return x.cmp(y),
            _ => (a, b) = (&a[1..], &b[1..]),
        }
    }
}

fn split_digits(bytes: &[u8]) -> (&[u8], &[u8]) {
    let end = bytes
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(bytes.len());
    bytes.split_at(end)
}

fn trim_zeros(digits: &[u8]) -> &[u8] {
    let start = digits
        .iter()
        .position(|byte| *byte != b'0')
        .unwrap_or(digits.len());
    &digits[start..]
}

pub fn visible_entries(listing: &Listing, query: &str, hidden: bool, order: Order) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    let mut entries: Vec<_> = listing
        .entries
        .iter()
        .filter(|entry| (hidden || !entry.hidden) && entry.sort_name.contains(&query))
        .cloned()
        .collect();
    if order.sort != Sort::Name {
        entries.sort_by(|a, b| compare(a, b, order.sort));
    }
    if order.reverse {
        entries.reverse();
    }
    // Folders stay first in both directions, as in Thunar and ranger.
    entries.sort_by_key(|entry| !entry.folder);
    entries
}

#[derive(Clone, PartialEq)]
pub struct Navigation {
    pub paths: Vec<PathBuf>,
    pub cursor: usize,
}

impl Navigation {
    pub fn new(path: PathBuf) -> Self {
        Self {
            paths: vec![path],
            cursor: 0,
        }
    }
    pub fn current(&self) -> &Path {
        &self.paths[self.cursor]
    }
    pub fn visit(&mut self, path: PathBuf) {
        if path == self.current() {
            return;
        }
        self.paths.truncate(self.cursor + 1);
        self.paths.push(path);
        self.cursor += 1;
    }
    pub fn back(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }
    pub fn forward(&mut self) {
        self.cursor = (self.cursor + 1).min(self.paths.len() - 1);
    }
}

pub fn home() -> PathBuf {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn initial_path() -> PathBuf {
    let options = crate::cli::options();
    let selected_parent = options
        .select
        .as_ref()
        .and_then(|path| path.parent())
        .map(PathBuf::from);
    selected_parent
        .or_else(|| options.directory.clone())
        .map(|path| resolve(&path, &std::env::current_dir().unwrap_or_else(|_| home())))
        .unwrap_or_else(home)
}

/// Resolves typed paths: `~`, `~/x`, absolute, `file://` URIs, or relative to `base`.
pub fn resolve(input: &Path, base: &Path) -> PathBuf {
    let text = input.to_string_lossy();
    if let Some(path) = url::Url::parse(&text)
        .ok()
        .filter(|url| url.scheme() == "file")
        .and_then(|url| url.to_file_path().ok())
    {
        return path;
    }
    if text == "~" {
        return home();
    }
    if let Some(relative) = text.strip_prefix("~/") {
        return home().join(relative);
    }
    base.join(input)
}

/// Shortens long names in the middle and keeps the extension: "holiday-pho…tos.jpg".
/// Blitz 0.2 does not apply `text-overflow: ellipsis`, so labels are cut here.
pub fn shorten(name: &str, limit: usize) -> String {
    let count = name.chars().count();
    if count <= limit || limit < 5 {
        return name.to_owned();
    }
    let tail = (limit - 1) / 3;
    let head = limit - 1 - tail;
    let start: String = name.chars().take(head).collect();
    let end: String = name.chars().skip(count - tail).collect();
    format!("{}\u{2026}{}", start.trim_end(), end.trim_start())
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let units = [("TB", 1_u64 << 40), ("GB", 1 << 30), ("MB", 1 << 20)];
    let (unit, divisor) = units
        .into_iter()
        .find(|(_, divisor)| bytes >= *divisor)
        .unwrap_or(("KB", 1 << 10));
    format!("{:.1} {unit}", bytes as f64 / divisor as f64)
}

pub(crate) fn format_time(time: SystemTime) -> String {
    let Ok(stamp) = jiff::Timestamp::try_from(time) else {
        return String::new();
    };
    let zone = jiff::tz::TimeZone::system();
    let local = stamp.to_zoned(zone.clone());
    let today = jiff::Timestamp::now().to_zoned(zone).date();
    if local.date() == today {
        local.strftime("Today %H:%M").to_string()
    } else if local.date().year() == today.year() {
        local.strftime("%b %-d %H:%M").to_string()
    } else {
        local.strftime("%b %-d %Y").to_string()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as Atomic};
    static ID: AtomicUsize = AtomicUsize::new(0);

    pub struct Fixture(pub PathBuf);
    impl Fixture {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "explorer-test-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Atomic::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|item| item.name.as_str()).collect()
    }

    #[test]
    fn scan_filter_sort_and_failure() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("Zebra")).unwrap();
        fs::write(fixture.0.join("alpha.txt"), "abc").unwrap();
        fs::write(fixture.0.join(".hidden"), "secret.txt\n").unwrap();
        fs::write(fixture.0.join("secret.txt"), "x").unwrap();
        fs::write(fixture.0.join("LARGE.txt"), "abcdefgh").unwrap();
        let listing = read_directory(&fixture.0).unwrap();
        let order = Order::default();
        let entries = visible_entries(&listing, "", false, order);
        assert_eq!(names(&entries), ["Zebra", "alpha.txt", "LARGE.txt"]);
        assert_eq!(visible_entries(&listing, " ALPHA ", true, order).len(), 1);
        assert_eq!(visible_entries(&listing, "", true, order).len(), 5);
        let size = Order {
            sort: Sort::Size,
            reverse: false,
        };
        assert_eq!(
            visible_entries(&listing, "", false, size)[1].name,
            "LARGE.txt"
        );
        let reversed = Order {
            reverse: true,
            ..order
        };
        assert_eq!(
            names(&visible_entries(&listing, "", false, reversed)),
            ["Zebra", "LARGE.txt", "alpha.txt"]
        );
        assert!(read_directory(&fixture.0.join("missing")).is_err());
    }

    #[test]
    fn long_names_keep_their_start_and_extension() {
        assert_eq!(shorten("short.txt", 20), "short.txt");
        assert_eq!(
            shorten("15599 - arab knife leila_khaled map.jpg", 20),
            "15599 - arab\u{2026}ap.jpg"
        );
        assert_eq!(shorten("ééééééééééé", 6), "éééé\u{2026}é");
        assert_eq!(shorten("abcdefgh", 3), "abcdefgh");
    }

    #[test]
    fn natural_order_compares_numbers_by_value() {
        let mut names = vec!["file10", "file2", "file02b", "file1", "a", "file"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["a", "file", "file1", "file2", "file02b", "file10"]);
        assert_eq!(natural("img007", "img7"), Ordering::Equal);
    }

    #[test]
    fn permissions_render_like_ls() {
        let fixture = Fixture::new();
        let file = fixture.0.join("script");
        fs::write(&file, "").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o754)).unwrap();
        let listing = read_directory(&fixture.0).unwrap();
        assert_eq!(listing.entries[0].permissions(), "-rwxr-xr--");
    }

    #[test]
    fn typed_paths_resolve_home_uris_and_relative_names() {
        let base = Path::new("/base");
        assert_eq!(resolve(Path::new("~"), base), home());
        assert_eq!(resolve(Path::new("~/x"), base), home().join("x"));
        assert_eq!(resolve(Path::new("child"), base), base.join("child"));
        assert_eq!(resolve(Path::new("/abs"), base), PathBuf::from("/abs"));
        assert_eq!(
            resolve(Path::new("file:///mnt/Big%20storage"), base),
            PathBuf::from("/mnt/Big storage")
        );
    }

    #[test]
    fn history_discards_forward_branch_and_stops_at_boundaries() {
        let mut nav = Navigation::new(PathBuf::from("/a"));
        nav.back();
        nav.visit(PathBuf::from("/b"));
        nav.visit(PathBuf::from("/c"));
        nav.back();
        nav.visit(PathBuf::from("/d"));
        nav.forward();
        assert_eq!(nav.current(), Path::new("/d"));
        assert_eq!(nav.paths.len(), 3);
        nav.visit(PathBuf::from("/d"));
        assert_eq!(nav.paths.len(), 3);
    }

    #[test]
    fn native_paths_and_directory_links_survive_display_conversion() {
        use std::os::unix::{ffi::OsStringExt, fs::symlink};
        let fixture = Fixture::new();
        let raw = fixture
            .0
            .join(std::ffi::OsString::from_vec(vec![b'a', 0xff]));
        fs::write(&raw, "native").unwrap();
        fs::create_dir(fixture.0.join("directory")).unwrap();
        symlink(fixture.0.join("directory"), fixture.0.join("link")).unwrap();
        symlink(fixture.0.join("missing"), fixture.0.join("broken")).unwrap();
        let listing = read_directory(&fixture.0).unwrap();
        assert!(listing.entries.iter().any(|entry| entry.path == raw));
        let find = |name| {
            listing
                .entries
                .iter()
                .find(|entry| entry.name == name)
                .unwrap()
        };
        assert!(find("link").folder && find("link").symlink);
        assert!(!find("broken").folder && find("broken").symlink);
        assert_eq!(listing.warnings, 0);
    }
}
