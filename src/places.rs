use crate::filesystem::home;
use std::{fs, path::PathBuf};

#[derive(Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub path: PathBuf,
}

impl Place {
    fn new(name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            name: name.into(),
            path,
        }
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct Places {
    /// The home folder, shown as the user's name like the reference sidebar.
    pub home: Option<Place>,
    pub folders: Vec<Place>,
    pub bookmarks: Vec<Place>,
    pub devices: Vec<Place>,
}

pub fn load() -> Places {
    let home = home();
    let user = home
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("Home"));
    let mut folders = Vec::new();
    if let Some(dirs) = directories::UserDirs::new() {
        for path in [
            dirs.desktop_dir(),
            dirs.document_dir(),
            dirs.download_dir(),
            dirs.audio_dir(),
            dirs.picture_dir(),
            dirs.video_dir(),
        ]
        .into_iter()
        .flatten()
        .filter(|path| *path != home)
        {
            folders.push(Place::new(name_of(path), path.to_path_buf()));
        }
    }
    let config = directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| home.join(".config"));
    let bookmarks = fs::read_to_string(config.join("gtk-3.0/bookmarks"))
        .map(|text| parse_bookmarks(&text))
        .unwrap_or_default();
    // Thunar lists bookmarks separately, but the reference sidebar shows each folder once.
    let devices = parse_mounts(&fs::read_to_string("/proc/self/mounts").unwrap_or_default());
    let listed = |path: &PathBuf| {
        *path == home
            || folders
                .iter()
                .chain(&devices)
                .any(|known| known.path == *path)
    };
    let bookmarks = bookmarks
        .into_iter()
        .filter(|place| !listed(&place.path))
        .collect();
    Places {
        home: Some(Place::new(user, home)),
        folders,
        bookmarks,
        devices,
    }
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Reads GTK bookmarks, the list Thunar shows in its sidebar.
pub fn parse_bookmarks(text: &str) -> Vec<Place> {
    text.lines()
        .filter_map(|line| {
            let (uri, label) = line.split_once(' ').unwrap_or((line, ""));
            let path = url::Url::parse(uri).ok()?.to_file_path().ok()?;
            let label = if label.trim().is_empty() {
                name_of(&path)
            } else {
                label.trim().to_owned()
            };
            Some(Place::new(label, path))
        })
        .collect()
}

/// Mounted user volumes: removable media and mounts under /mnt.
pub fn parse_mounts(text: &str) -> Vec<Place> {
    let mut places: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let point = unescape_mount(line.split_whitespace().nth(1)?);
            let path = PathBuf::from(&point);
            let removable = ["/run/media/", "/media/"]
                .iter()
                .any(|prefix| point.starts_with(prefix));
            let mounted = point.starts_with("/mnt/") && path.components().count() == 3;
            (removable || mounted).then(|| Place::new(name_of(&path), path))
        })
        .collect();
    places.dedup_by(|a, b| a.path == b.path);
    places
}

/// /proc/self/mounts encodes spaces and tabs as octal escapes.
fn unescape_mount(field: &str) -> String {
    field
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\134", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks_use_labels_or_folder_names() {
        let places = parse_bookmarks(
            "file:///mnt/Big%20storage/Ebooks Books\nfile:///home/u/projects\nsftp://host/x\n",
        );
        assert_eq!(places.len(), 2);
        assert_eq!(places[0].name, "Books");
        assert_eq!(places[0].path, PathBuf::from("/mnt/Big storage/Ebooks"));
        assert_eq!(places[1].name, "projects");
    }

    #[test]
    fn mounts_keep_user_volumes_only() {
        let places = parse_mounts(
            "/dev/sda1 / ext4 rw 0 0\n\
             /dev/sdb1 /mnt/Big\\040storage ntfs3 rw 0 0\n\
             /dev/sdc1 /run/media/u/USB fat rw 0 0\n\
             /dev/sdd1 /mnt/a/b ext4 rw 0 0\n\
             proc /proc proc rw 0 0\n",
        );
        let names: Vec<_> = places.iter().map(|place| place.name.as_str()).collect();
        assert_eq!(names, ["Big storage", "USB"]);
    }
}
