use crate::filesystem::Entry;
use futures_util::StreamExt;
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

const SERVICE: &str = "org.freedesktop.thumbnails.Thumbnailer1";
const OBJECT: &str = "/org/freedesktop/thumbnails/Thumbnailer1";

/// Tumbler flavors by their largest edge: 128, 256, and 512 px.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Flavor {
    Normal,
    Large,
    XLarge,
}

impl Flavor {
    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Large => "large",
            Self::XLarge => "x-large",
        }
    }

    /// The smallest flavor that covers `pixels`. Smaller images decode into less
    /// memory and are cheaper to draw each frame, especially with the CPU renderer.
    pub fn covering(pixels: f64) -> Self {
        if pixels <= 128.0 {
            Self::Normal
        } else if pixels <= 256.0 {
            Self::Large
        } else {
            Self::XLarge
        }
    }
}

pub fn cache_path(uri: &str, flavor: Flavor) -> PathBuf {
    let cache = directories::BaseDirs::new()
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| crate::filesystem::home().join(".cache"));
    cache
        .join("thumbnails")
        .join(flavor.name())
        .join(format!("{:x}.png", md5::compute(uri.as_bytes())))
}

fn valid_cache(original: &Path, thumbnail: &Path, uri: &str) -> bool {
    let Some(mtime) = File::open(original)
        .ok()
        .and_then(|file| file.metadata().ok())
        .and_then(|data| data.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
    else {
        return false;
    };
    let Some(reader) = File::open(thumbnail)
        .ok()
        .and_then(|file| png::Decoder::new(BufReader::new(file)).read_info().ok())
    else {
        return false;
    };
    let text = &reader.info().uncompressed_latin1_text;
    text.iter()
        .any(|chunk| chunk.keyword == "Thumb::URI" && chunk.text == uri)
        && text
            .iter()
            .any(|chunk| chunk.keyword == "Thumb::MTime" && mtime_matches(&chunk.text, mtime))
}

fn mtime_matches(stamp: &str, modified: Duration) -> bool {
    let (seconds, fraction) = stamp
        .split_once('.')
        .map_or((stamp, None), |(seconds, fraction)| {
            (seconds, Some(fraction))
        });
    if seconds.parse::<u64>().ok() != Some(modified.as_secs()) {
        return false;
    }
    let Some(fraction) = fraction else {
        return true;
    };
    if fraction.is_empty() || fraction.len() > 9 {
        return false;
    }
    let scale = 10_u32.pow(9 - fraction.len() as u32);
    fraction.parse::<u32>().ok() == Some(modified.subsec_nanos() / scale)
}

fn cached_images(entries: &[Entry], flavor: Flavor) -> HashMap<PathBuf, String> {
    entries
        .iter()
        .filter_map(|entry| {
            let uri = entry.uri();
            let thumbnail = cache_path(&uri, flavor);
            if !valid_cache(&entry.path, &thumbnail, &uri) {
                return None;
            }
            url::Url::from_file_path(thumbnail)
                .ok()
                .map(|uri| (entry.path.clone(), uri.to_string()))
        })
        .collect()
}

/// Only image files belong in gallery mode; videos and documents stay in normal views.
pub fn is_image(entry: &Entry) -> bool {
    !entry.folder
        && mime_guess::from_path(&entry.path)
            .first()
            .is_some_and(|mime| mime.type_().as_str() == "image")
}

pub fn can_preview(entry: &Entry) -> bool {
    !entry.folder
        && mime_guess::from_path(&entry.path)
            .first()
            .is_some_and(|mime| {
                ["image", "video", "audio", "font"].contains(&mime.type_().as_str())
                    || ["application/pdf", "application/epub+zip"].contains(&mime.essence_str())
            })
}

/// Reports cached thumbnails at once, then asks Tumbler for the rest and reports again.
pub async fn load(
    entries: Vec<Entry>,
    flavor: Flavor,
    mut update: impl FnMut(HashMap<PathBuf, String>),
) -> Result<(), String> {
    let entries: Vec<_> = entries.into_iter().filter(can_preview).collect();
    let for_cache = entries.clone();
    let cached = tokio::task::spawn_blocking(move || cached_images(&for_cache, flavor))
        .await
        .map_err(|error| error.to_string())?;
    let missing: Vec<_> = entries
        .iter()
        .filter(|entry| !cached.contains_key(&entry.path))
        .cloned()
        .collect();
    update(cached);
    if missing.is_empty() {
        return Ok(());
    }
    let result = tokio::time::timeout(Duration::from_secs(15), request(&missing, flavor))
        .await
        .map_err(|_| String::from("Thumbnail service timed out"))?;
    let cached = tokio::task::spawn_blocking(move || cached_images(&entries, flavor))
        .await
        .map_err(|error| error.to_string())?;
    update(cached);
    result.map_err(|error| format!("Tumbler: {error}"))
}

async fn request(entries: &[Entry], flavor: Flavor) -> Result<(), zbus::Error> {
    let connection = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&connection, SERVICE, OBJECT, SERVICE).await?;
    let (schemes, types): (Vec<String>, Vec<String>) = proxy.call("GetSupported", &()).await?;
    let mut uris = Vec::new();
    let mut mime_types = Vec::new();
    for entry in entries {
        let mime = mime_guess::from_path(&entry.path)
            .first_or_octet_stream()
            .to_string();
        if !schemes
            .iter()
            .zip(&types)
            .any(|(scheme, kind)| scheme == "file" && *kind == mime)
        {
            continue;
        }
        uris.push(entry.uri());
        mime_types.push(mime);
    }
    if uris.is_empty() {
        return Ok(());
    }
    // Subscribe before Queue so a warm-cache Finished signal cannot be missed.
    let mut finished = proxy.receive_signal("Finished").await?;
    let handle: u32 = proxy
        .call(
            "Queue",
            &(uris, mime_types, flavor.name(), "foreground", 0_u32),
        )
        .await?;
    while let Some(message) = finished.next().await {
        if message.body().deserialize::<u32>()? == handle {
            return Ok(());
        }
    }
    Err(zbus::Error::Failure(String::from(
        "Thumbnail service disconnected",
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_filters_image_mimes_and_skips_folders() {
        use crate::filesystem::{read_directory, tests::Fixture};
        let fixture = Fixture::new();
        for name in ["frame.jpg", "sketch.png", "movie.mp4", "notes.txt"] {
            std::fs::write(fixture.0.join(name), "").unwrap();
        }
        std::fs::create_dir(fixture.0.join("other.png")).unwrap();
        let entries = read_directory(&fixture.0).unwrap().entries;
        let images: Vec<_> = entries.iter().filter(|entry| is_image(entry)).map(|entry| entry.name.clone()).collect();
        assert_eq!(images, ["frame.jpg", "sketch.png"]);
    }

    #[test]
    fn tumbler_fractional_timestamps_and_standard_seconds_are_validated() {
        let modified = Duration::new(123, 452_754_500);
        assert!(mtime_matches("123", modified));
        assert!(mtime_matches("123.452754", modified));
        assert!(!mtime_matches("123.452755", modified));
        assert!(!mtime_matches("124", modified));
        assert!(!mtime_matches("123.invalid", modified));
        assert!(!mtime_matches("123.4527545000", modified));
    }
    #[test]
    fn flavor_covers_the_drawn_size() {
        assert_eq!(Flavor::covering(128.0), Flavor::Normal);
        assert_eq!(Flavor::covering(160.0), Flavor::Large);
        assert_eq!(Flavor::covering(288.0), Flavor::XLarge);
    }

    #[test]
    fn cache_name_uses_the_uri_not_file_contents() {
        assert_eq!(
            cache_path("file:///home/jens/photos/me.png", Flavor::Large)
                .file_name()
                .unwrap(),
            "c6ee772d9e49320e97ec29a7eb5b1697.png"
        );
    }

    #[test]
    fn cache_metadata_rejects_changed_sources_and_invalid_pngs() {
        let root = std::env::temp_dir().join(format!("explorer-cache-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let original = root.join("original");
        let cached = root.join("cached.png");
        std::fs::write(&original, "source").unwrap();
        let uri = url::Url::from_file_path(&original).unwrap().to_string();
        let mtime = std::fs::metadata(&original)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        let mut encoder = png::Encoder::new(File::create(&cached).unwrap(), 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .add_text_chunk("Thumb::URI".into(), uri.clone())
            .unwrap();
        encoder
            .add_text_chunk("Thumb::MTime".into(), mtime)
            .unwrap();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[0, 0, 0])
            .unwrap();
        assert!(valid_cache(&original, &cached, &uri));
        assert!(!valid_cache(&original, &cached, "file:///different"));
        File::options()
            .write(true)
            .open(&original)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(UNIX_EPOCH))
            .unwrap();
        assert!(!valid_cache(&original, &cached, &uri));
        std::fs::write(&cached, "invalid").unwrap();
        assert!(!valid_cache(&original, &cached, &uri));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "Requires the session Tumbler service"]
    fn tumbler_creates_a_valid_shared_thumbnail() {
        let source =
            std::env::temp_dir().join(format!("explorer-tumbler-test-{}.png", std::process::id()));
        let mut encoder = png::Encoder::new(File::create(&source).unwrap(), 32, 32);
        encoder.set_color(png::ColorType::Rgb);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![100; 32 * 32 * 3])
            .unwrap();
        let entry = crate::filesystem::read_directory(source.parent().unwrap())
            .unwrap()
            .entries
            .into_iter()
            .find(|entry| entry.path == source)
            .unwrap();
        let mut images = HashMap::new();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(load(vec![entry.clone()], Flavor::Large, |batch| {
                images = batch
            }))
            .unwrap();
        assert!(images.contains_key(&source));
        let cached = cache_path(&entry.uri(), Flavor::Large);
        assert!(valid_cache(&source, &cached, &entry.uri()));
        std::fs::remove_file(cached).unwrap();
        std::fs::remove_file(source).unwrap();
    }
}
