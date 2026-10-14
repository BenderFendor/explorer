//! Files on the Wayland clipboard, in the formats GTK file managers use:
//! `x-special/gnome-copied-files` (Thunar, Nautilus, Nemo), `text/uri-list` (most apps),
//! and plain paths for text fields and terminals.

use crate::operations::Transfer;
use std::{
    io::Read,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use url::Url;
use wl_clipboard_rs::{copy, paste};

const GNOME: &str = "x-special/gnome-copied-files";
const URI_LIST: &str = "text/uri-list";

/// Only the windowed app uses the session clipboard; tests and captures never touch it.
static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn uri(path: &std::path::Path) -> Option<String> {
    Url::from_file_path(path).ok().map(String::from)
}

pub struct Encoded {
    pub gnome: String,
    pub uri_list: String,
    pub text: String,
}

pub fn encode(paths: &[PathBuf], mode: Transfer) -> Encoded {
    let uris: Vec<String> = paths.iter().filter_map(|path| uri(path)).collect();
    let verb = if mode == Transfer::Move {
        "cut"
    } else {
        "copy"
    };
    Encoded {
        gnome: std::iter::once(verb.to_owned())
            .chain(uris.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n"),
        uri_list: uris.iter().map(|uri| format!("{uri}\r\n")).collect(),
        text: paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn file_paths<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<PathBuf> {
    lines
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| Url::parse(line).ok()?.to_file_path().ok())
        .collect()
}

pub fn decode_gnome(text: &str) -> Option<(Transfer, Vec<PathBuf>)> {
    let mut lines = text.lines();
    let mode = match lines.next()?.trim() {
        "cut" => Transfer::Move,
        "copy" => Transfer::Copy,
        _ => return None,
    };
    Some((mode, file_paths(lines)))
}

pub fn decode_uri_list(text: &str) -> Vec<PathBuf> {
    file_paths(text.lines())
}

/// Plain text pastes count only when every line is an existing absolute path.
pub fn decode_text(text: &str) -> Vec<PathBuf> {
    let paths: Vec<PathBuf> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();
    if !paths.is_empty() && paths.iter().all(|path| path.is_absolute() && path.exists()) {
        paths
    } else {
        Vec::new()
    }
}

/// Offers the files on the clipboard. The data is served from a background thread
/// until another client takes the clipboard.
pub fn copy(paths: &[PathBuf], mode: Transfer) -> Result<(), String> {
    let encoded = encode(paths, mode);
    let offer = |mime: &str, data: String| copy::MimeSource {
        source: copy::Source::Bytes(data.into_bytes().into_boxed_slice()),
        mime_type: copy::MimeType::Specific(mime.to_owned()),
    };
    let sources = vec![
        offer(GNOME, encoded.gnome),
        offer(URI_LIST, encoded.uri_list),
        offer("text/plain;charset=utf-8", encoded.text.clone()),
        offer("text/plain", encoded.text),
    ];
    copy::Options::new()
        .copy_multi(sources)
        .map_err(|error| format!("Clipboard: {error}"))
}

#[derive(Debug, PartialEq)]
pub enum Payload {
    Files(Transfer, Vec<PathBuf>),
    Bytes { name: String, data: Vec<u8> },
    Link(String),
}

const IMAGES: &[(&str, &str)] = &[
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
    ("image/bmp", "bmp"),
    ("image/tiff", "tiff"),
    ("image/svg+xml", "svg"),
];

fn text_payload(text: String) -> Option<Payload> {
    let paths = decode_text(&text);
    if !paths.is_empty() {
        Some(Payload::Files(Transfer::Copy, paths))
    } else if crate::imports::url(text.trim()).is_ok() {
        Some(Payload::Link(text.trim().to_owned()))
    } else if text.is_empty() {
        None
    } else {
        Some(Payload::Bytes {
            name: "clipboard.txt".into(),
            data: text.into_bytes(),
        })
    }
}

/// Prefer file-manager formats, then encoded images, then text or a download URL.
pub fn paste() -> Result<Option<Payload>, String> {
    let offered =
        match paste::get_mime_types(paste::ClipboardType::Regular, paste::Seat::Unspecified) {
            Ok(types) => types,
            Err(paste::Error::ClipboardEmpty | paste::Error::NoSeats) => return Ok(None),
            Err(error) => return Err(format!("Clipboard: {error}")),
        };
    let read = |mime: &str| -> Result<Vec<u8>, String> {
        let (mut pipe, _) = paste::get_contents(
            paste::ClipboardType::Regular,
            paste::Seat::Unspecified,
            paste::MimeType::Specific(mime),
        )
        .map_err(|error| format!("Clipboard: {error}"))?;
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)
            .map_err(|error| format!("Clipboard: {error}"))?;
        Ok(bytes)
    };
    if let Some(mime) = [GNOME, URI_LIST]
        .into_iter()
        .find(|mime| offered.contains(*mime))
    {
        let text = String::from_utf8(read(mime)?).map_err(|error| error.to_string())?;
        let found = if mime == GNOME {
            decode_gnome(&text)
        } else {
            Some((Transfer::Copy, decode_uri_list(&text)))
        };
        if let Some((mode, paths)) = found.filter(|(_, paths)| !paths.is_empty()) {
            return Ok(Some(Payload::Files(mode, paths)));
        }
        if let Some(link) = text
            .lines()
            .find(|line| crate::imports::url(line.trim()).is_ok())
        {
            return Ok(Some(Payload::Link(link.trim().to_owned())));
        }
    }
    if let Some((mime, extension)) = IMAGES.iter().find(|(mime, _)| offered.contains(*mime)) {
        return Ok(Some(Payload::Bytes {
            name: format!("clipboard.{extension}"),
            data: read(mime)?,
        }));
    }
    let mime = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"]
        .into_iter()
        .find(|mime| offered.contains(*mime));
    match mime {
        Some(mime) => Ok(text_payload(
            String::from_utf8(read(mime)?).map_err(|error| error.to_string())?,
        )),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_round_trip_with_spaces() {
        let paths = vec![
            PathBuf::from("/mnt/Big storage/a b.txt"),
            PathBuf::from("/tmp/x#1"),
        ];
        let encoded = encode(&paths, Transfer::Move);
        assert!(
            encoded
                .gnome
                .starts_with("cut\nfile:///mnt/Big%20storage/a%20b.txt\n")
        );
        assert_eq!(
            decode_gnome(&encoded.gnome),
            Some((Transfer::Move, paths.clone()))
        );
        assert_eq!(decode_uri_list(&encoded.uri_list), paths);
        assert_eq!(
            encode(&paths, Transfer::Copy).text,
            "/mnt/Big storage/a b.txt\n/tmp/x#1"
        );
    }

    #[test]
    fn foreign_formats_are_parsed_defensively() {
        assert_eq!(
            decode_gnome("copy\nfile:///a\n"),
            Some((Transfer::Copy, vec![PathBuf::from("/a")]))
        );
        assert_eq!(decode_gnome("move\nfile:///a"), None);
        assert_eq!(
            decode_uri_list("# comment\r\nhttps://example.com\r\nfile:///b\r\n"),
            [PathBuf::from("/b")]
        );
        assert!(decode_text("hello world").is_empty());
        assert!(decode_text("relative/path").is_empty());
        assert_eq!(decode_text("/\n"), [PathBuf::from("/")]);
    }

    #[test]
    fn plain_text_is_preserved_and_single_http_urls_download() {
        assert_eq!(
            text_payload("line one\nline two\n".into()),
            Some(Payload::Bytes {
                name: "clipboard.txt".into(),
                data: b"line one\nline two\n".to_vec()
            })
        );
        assert_eq!(
            text_payload(" https://example.org/photo.png ".into()),
            Some(Payload::Link("https://example.org/photo.png".into()))
        );
        assert_eq!(text_payload(String::new()), None);
    }

    #[test]
    #[ignore = "Writes and reads the session's real Wayland clipboard"]
    fn session_clipboard_round_trip() {
        let paths = vec![std::env::temp_dir()];
        copy(&paths, Transfer::Copy).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(
            paste().unwrap(),
            Some(Payload::Files(Transfer::Copy, paths))
        );
        // Encoded images and text from another client are preserved byte for byte.
        for (mime, data, name) in [
            ("image/png", b"PNG bytes".as_slice(), "clipboard.png"),
            ("text/plain", b"hello\nworld", "clipboard.txt"),
        ] {
            copy::Options::new()
                .copy(
                    copy::Source::Bytes(data.to_vec().into_boxed_slice()),
                    copy::MimeType::Specific(mime.into()),
                )
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
            assert_eq!(
                paste().unwrap(),
                Some(Payload::Bytes {
                    name: name.into(),
                    data: data.to_vec()
                })
            );
        }
        copy(&[std::env::temp_dir()], Transfer::Copy).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        // Another client sees the GTK formats.
        let types = std::process::Command::new("wl-paste")
            .arg("--list-types")
            .output()
            .unwrap();
        let types = String::from_utf8_lossy(&types.stdout);
        assert!(types.contains(GNOME) && types.contains(URI_LIST), "{types}");
        // A cut made by another file manager arrives as a move.
        let mut wl_copy = std::process::Command::new("wl-copy")
            .args(["--type", GNOME])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        wl_copy
            .stdin
            .take()
            .unwrap()
            .write_all(b"cut\nfile:///tmp")
            .unwrap();
        wl_copy.wait().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(
            paste().unwrap(),
            Some(Payload::Files(Transfer::Move, vec![PathBuf::from("/tmp")]))
        );
    }
}
