use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Transfer {
    Copy,
    Move,
}

#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn fraction(&self) -> f64 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.done.load(Ordering::Relaxed) as f64 / total as f64).min(1.0)
    }
    fn advance(&self, bytes: u64) -> io::Result<()> {
        self.done.fetch_add(bytes, Ordering::Relaxed);
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
        }
        Ok(())
    }
}

/// Thunar's rule for drops: move within one filesystem, copy across filesystems.
/// Returns None when the drop would change nothing or place a folder inside itself.
pub fn drop_mode(sources: &[PathBuf], folder: &Path) -> Option<Transfer> {
    use std::os::unix::fs::MetadataExt;
    if sources.iter().all(|source| source.parent() == Some(folder))
        || sources.iter().any(|source| folder.starts_with(source))
    {
        return None;
    }
    let device = |path: &Path| fs::metadata(path).map(|data| data.dev()).ok();
    let target = device(folder)?;
    let same = sources
        .iter()
        .all(|source| source.parent().and_then(device) == Some(target));
    Some(if same { Transfer::Move } else { Transfer::Copy })
}

/// Copies or moves `sources` into `folder`. Existing names are never overwritten:
/// a conflicting item receives a "(copy)" name, as Thunar does for same-folder pastes.
pub fn transfer(
    sources: &[PathBuf],
    folder: &Path,
    mode: Transfer,
    progress: &Progress,
) -> Result<Vec<PathBuf>, String> {
    for source in sources {
        if folder.starts_with(source) {
            return Err(format!(
                "Cannot place {} inside itself",
                display_name(source)
            ));
        }
    }
    let total = sources.iter().map(|source| tree_size(source)).sum();
    progress.total.store(total, Ordering::Relaxed);
    let mut created = Vec::new();
    for source in sources {
        if mode == Transfer::Move && source.parent() == Some(folder) {
            continue;
        }
        let name = source
            .file_name()
            .ok_or_else(|| format!("Cannot copy {}", source.display()))?;
        let target = available_name(folder, name);
        let result = match mode {
            Transfer::Copy => copy_new(source, &target, progress),
            Transfer::Move => move_item(source, &target, progress),
        };
        result.map_err(|error| format!("{}: {error}", display_name(source)))?;
        if source.is_file() && target.is_file() {
            let preserved = match mode {
                Transfer::Copy => crate::image_metadata::copy_sidecar(source, &target),
                Transfer::Move => crate::image_metadata::move_sidecar(source, &target),
            };
            preserved.map_err(|error| {
                format!(
                    "{} transferred but metadata could not be preserved: {error}",
                    display_name(source)
                )
            })?;
        }
        if mode == Transfer::Move {
            crate::undo::record_move(source, &target);
        } else {
            crate::undo::record_add(&target);
        }
        created.push(target);
    }
    Ok(created)
}

fn move_item(source: &Path, target: &Path, progress: &Progress) -> io::Result<()> {
    match fs::rename(source, target) {
        Ok(()) => progress.advance(tree_size(source).max(tree_size(target))),
        // Different filesystems need a copy before the source is removed.
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            copy_new(source, target, progress)?;
            remove(source)
        }
        Err(error) => Err(error),
    }
}

/// Copies into a target that did not exist, removing the partial copy on failure.
fn copy_new(source: &Path, target: &Path, progress: &Progress) -> io::Result<()> {
    copy_tree(source, target, progress).inspect_err(|_| {
        let _ = remove(target);
    })
}

fn copy_tree(source: &Path, target: &Path, progress: &Progress) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(source)?, target)?;
        return progress.advance(0);
    }
    if metadata.is_dir() {
        fs::create_dir(target)?;
        for child in fs::read_dir(source)? {
            let child = child?;
            copy_tree(&child.path(), &target.join(child.file_name()), progress)?;
        }
        fs::set_permissions(target, metadata.permissions())?;
        return progress.advance(0);
    }
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    drop(file);
    fs::copy(source, target)?;
    if let Ok(modified) = metadata.modified() {
        fs::File::options()
            .write(true)
            .open(target)?
            .set_modified(modified)?;
    }
    progress.advance(metadata.len())
}

fn tree_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    fs::read_dir(path)
        .map(|children| {
            children
                .filter_map(Result::ok)
                .map(|child| tree_size(&child.path()))
                .sum()
        })
        .unwrap_or(0)
}

/// Returns `folder/name`, or the first free "name (copy).ext", "name (copy 2).ext", …
pub fn available_name(folder: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let candidate = folder.join(name);
    if fs::symlink_metadata(&candidate).is_err() {
        return candidate;
    }
    let path = Path::new(name);
    let is_dir = fs::metadata(&candidate).is_ok_and(|data| data.is_dir());
    let (stem, extension) = match (path.file_stem(), path.extension()) {
        (Some(stem), Some(ext)) if !is_dir => (stem.to_owned(), Some(ext.to_owned())),
        _ => (name.to_owned(), None),
    };
    (1..)
        .map(|index| {
            let mut name = stem.clone();
            name.push(if index == 1 {
                String::from(" (copy)")
            } else {
                format!(" (copy {index})")
            });
            if let Some(extension) = &extension {
                name.push(".");
                name.push(extension);
            }
            folder.join(name)
        })
        .find(|path| fs::symlink_metadata(path).is_err())
        .expect("unbounded candidate names")
}

/// Removes a file, link, or folder without following links.
pub fn remove(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

pub fn delete(paths: &[PathBuf]) -> Result<(), String> {
    paths.iter().try_for_each(|path| {
        remove(path).map_err(|error| format!("{}: {error}", display_name(path)))
    })
}

/// Moves items to the freedesktop trash through GLib, which Thunar also uses.
pub fn trash(paths: &[PathBuf]) -> Result<(), String> {
    gio(std::iter::once(OsString::from("trash")).chain(paths.iter().map(|path| path.into())))
}

pub fn restore(paths: &[PathBuf]) -> Result<(), String> {
    let uris = paths.iter().filter_map(|path| {
        let name = path.file_name()?.to_str()?;
        Some(OsString::from(format!("trash:///{}", urlencode(name))))
    });
    gio(["trash", "--restore"]
        .map(OsString::from)
        .into_iter()
        .chain(uris))
}

pub fn empty_trash() -> Result<(), String> {
    gio(["trash", "--empty"].map(OsString::from))
}

fn urlencode(name: &str) -> String {
    url::form_urlencoded::byte_serialize(name.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

fn gio(arguments: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let output = Command::new("gio")
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("gio: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

pub fn trash_files() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| crate::filesystem::home().join(".local/share"))
        .join("Trash/files")
}

pub(crate) fn valid_name(name: &str) -> Result<&str, String> {
    let name = name.trim_end_matches('\n');
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(format!("\"{name}\" is not a valid name"));
    }
    Ok(name)
}

pub fn rename(path: &Path, name: &str) -> Result<PathBuf, String> {
    let target = path.with_file_name(valid_name(name)?);
    if target == path {
        return Ok(target);
    }
    if fs::symlink_metadata(&target).is_ok() {
        return Err(format!("{name} already exists"));
    }
    fs::rename(path, &target).map_err(|error| error.to_string())?;
    if let Err(error) = crate::image_metadata::move_sidecar(path, &target) {
        return Err(format!(
            "Renamed file, but metadata could not be relocated: {error}"
        ));
    }
    crate::undo::record_move(path,&target);
    Ok(target)
}

pub fn create(folder: &Path, name: &str, directory: bool) -> Result<PathBuf, String> {
    let target = folder.join(valid_name(name)?);
    let result = if directory {
        fs::create_dir(&target)
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map(drop)
    };
    result.map_err(|error| format!("{name}: {error}"))?;
    crate::undo::record_add(&target);
    Ok(target)
}

pub fn open(path: &Path) -> Result<(), String> {
    spawn(Command::new("xdg-open").arg(path))
}

pub fn terminal(folder: &Path) -> Result<(), String> {
    let program = std::env::var_os("TERMINAL").unwrap_or_else(|| "xdg-terminal-exec".into());
    spawn(Command::new(program).current_dir(folder))
}

/// Runs a shell command in `folder` with the selected files as "$@", like ranger's open_with.
pub fn run_with(command: &str, folder: &Path, files: &[PathBuf]) -> Result<(), String> {
    let script = if command.contains("$@") || command.contains("$1") {
        command.to_owned()
    } else {
        format!("{command} \"$@\"")
    };
    spawn(
        Command::new("sh")
            .arg("-c")
            .arg(script)
            .arg("explorer")
            .args(files)
            .current_dir(folder),
    )
}

fn spawn(command: &mut Command) -> Result<(), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|error| format!("{}: {error}", command.get_program().to_string_lossy()))
}

pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::tests::Fixture;

    fn tree(root: &Path) {
        fs::create_dir_all(root.join("album/inner")).unwrap();
        fs::write(root.join("album/a.txt"), "aaaa").unwrap();
        fs::write(root.join("album/inner/b.txt"), "bb").unwrap();
        std::os::unix::fs::symlink("a.txt", root.join("album/link")).unwrap();
    }

    #[test]
    fn copy_into_same_folder_creates_numbered_copies() {
        let fixture = Fixture::new();
        tree(&fixture.0);
        fs::write(fixture.0.join("note.txt"), "n").unwrap();
        let progress = Progress::default();
        let sources = [fixture.0.join("album"), fixture.0.join("note.txt")];
        let first = transfer(&sources, &fixture.0, Transfer::Copy, &progress).unwrap();
        let second = transfer(&sources, &fixture.0, Transfer::Copy, &progress).unwrap();
        assert_eq!(
            first,
            [
                fixture.0.join("album (copy)"),
                fixture.0.join("note (copy).txt")
            ]
        );
        assert_eq!(second[1], fixture.0.join("note (copy 2).txt"));
        let copy = fixture.0.join("album (copy)");
        assert_eq!(fs::read_to_string(copy.join("inner/b.txt")).unwrap(), "bb");
        assert_eq!(
            fs::read_link(copy.join("link")).unwrap(),
            Path::new("a.txt")
        );
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn move_relocates_and_refuses_recursion() {
        let fixture = Fixture::new();
        tree(&fixture.0);
        let target = fixture.0.join("target");
        fs::create_dir(&target).unwrap();
        let progress = Progress::default();
        let album = fixture.0.join("album");
        assert!(
            transfer(
                std::slice::from_ref(&album),
                &album.join("inner"),
                Transfer::Move,
                &progress
            )
            .is_err()
        );
        transfer(
            std::slice::from_ref(&album),
            &target,
            Transfer::Move,
            &progress,
        )
        .unwrap();
        assert!(!album.exists());
        assert_eq!(
            fs::read_to_string(target.join("album/a.txt")).unwrap(),
            "aaaa"
        );
        // Moving into the item's own folder is a no-op, not a duplicate.
        let moved = transfer(&[target.join("album")], &target, Transfer::Move, &progress).unwrap();
        assert!(moved.is_empty());
    }

    #[test]
    fn copy_keeps_modification_time_and_cancels_cleanly() {
        let fixture = Fixture::new();
        tree(&fixture.0);
        let source = fixture.0.join("album/a.txt");
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let out = fixture.0.join("out");
        fs::create_dir(&out).unwrap();
        transfer(&[source], &out, Transfer::Copy, &Progress::default()).unwrap();
        assert_eq!(
            fs::metadata(out.join("a.txt")).unwrap().modified().unwrap(),
            old
        );
        let cancelled = Progress::default();
        cancelled.cancel.store(true, Ordering::Relaxed);
        assert!(transfer(&[fixture.0.join("album")], &out, Transfer::Copy, &cancelled).is_err());
        assert!(!out.join("album").exists(), "partial copies are removed");
    }

    #[test]
    fn rename_and_create_validate_names_without_overwriting() {
        let fixture = Fixture::new();
        let file = create(&fixture.0, "draft.md", false).unwrap();
        create(&fixture.0, "folder", true).unwrap();
        assert!(create(&fixture.0, "folder", true).is_err());
        assert!(rename(&file, "folder").is_err());
        assert!(rename(&file, "a/b").is_err());
        assert!(rename(&file, "").is_err());
        let renamed = rename(&file, "final.md").unwrap();
        assert!(renamed.exists() && !file.exists());
        delete(&[renamed.clone(), fixture.0.join("folder")]).unwrap();
        assert!(!renamed.exists());
    }

    #[test]
    fn delete_removes_links_not_their_targets() {
        let fixture = Fixture::new();
        tree(&fixture.0);
        std::os::unix::fs::symlink(fixture.0.join("album"), fixture.0.join("shortcut")).unwrap();
        delete(&[fixture.0.join("shortcut")]).unwrap();
        assert!(fixture.0.join("album/a.txt").exists());
    }

    #[test]
    #[ignore = "Uses the session's real freedesktop trash through gio"]
    fn trash_and_restore_round_trip() {
        let name = format!("explorer trash test {}.txt", std::process::id());
        let original = crate::filesystem::home().join(&name);
        fs::write(&original, "round trip").unwrap();
        trash(std::slice::from_ref(&original)).unwrap();
        assert!(!original.exists());
        let trashed = trash_files().join(&name);
        assert!(trashed.exists(), "missing {}", trashed.display());
        restore(std::slice::from_ref(&trashed)).unwrap();
        assert_eq!(fs::read_to_string(&original).unwrap(), "round trip");
        assert!(!trashed.exists());
        fs::remove_file(original).unwrap();
    }

    #[test]
    fn drops_move_within_a_filesystem_and_skip_no_ops() {
        let fixture = Fixture::new();
        tree(&fixture.0);
        let album = fixture.0.join("album");
        let note = album.join("a.txt");
        assert_eq!(
            drop_mode(std::slice::from_ref(&note), &fixture.0),
            Some(Transfer::Move)
        );
        assert_eq!(drop_mode(std::slice::from_ref(&note), &album), None);
        assert_eq!(
            drop_mode(std::slice::from_ref(&album), &album.join("inner")),
            None
        );
        let other = Path::new("/proc");
        assert_eq!(
            drop_mode(std::slice::from_ref(&note), other),
            Some(Transfer::Copy)
        );
    }

    #[test]
    fn trash_uris_escape_names() {
        assert_eq!(urlencode("Big storage#1.txt"), "Big%20storage%231.txt");
    }
}

/// Used by the journal: move without overwriting and without recording a new action.
/// Keeps companion image annotations in sync, including across volumes.
pub fn move_exact(from:&Path,to:&Path)->Result<(),String>{
    if fs::symlink_metadata(to).is_ok(){return Err(format!("Undo destination already exists: {}",to.display()));}
    if !from.exists(){return Err(format!("Undo source missing: {}",from.display()));}
    let progress=Progress::default();
    move_item(from,to,&progress).map_err(|e|e.to_string())?;
    if from.is_file() || to.is_file() {
        crate::image_metadata::move_sidecar(from,to).map_err(|e|e.to_string())?;
    }
    Ok(())
}
