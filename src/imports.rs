//! Clipboard bytes and HTTP downloads, staged in the destination and published without overwrites.
use crate::operations::{self, Progress};
use std::{
    fs,
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use url::Url;

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, PartialEq)]
pub struct Pending {
    directory: PathBuf,
    pub suggested: String,
}

impl Drop for Pending {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

impl Pending {
    fn new(folder: &Path) -> Result<Self, String> {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let directory = folder.join(format!(".explorer-import-{}-{serial}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            suggested: String::from("download"),
        })
    }

    /// Hard linking within the folder publishes the complete file atomically.
    pub fn save(&self, name: &str) -> Result<PathBuf, String> {
        let name = operations::valid_name(name)?;
        let folder = self
            .directory
            .parent()
            .ok_or("Missing destination folder")?;
        loop {
            let target = operations::available_name(folder, name.as_ref());
            match fs::hard_link(self.directory.join("data"), &target) {
                Ok(()) => return Ok(target),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

pub fn bytes(folder: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let pending = Pending::new(folder)?;
    fs::File::create(pending.directory.join("data"))
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| e.to_string())?;
    pending.save(name)
}

pub fn url(text: &str) -> Result<Url, String> {
    let url = Url::parse(text).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Download needs an http:// or https:// URL".into());
    }
    Ok(url)
}

pub enum Download {
    Saved(PathBuf),
    Name(Pending),
}

pub fn download(
    folder: &Path,
    address: &str,
    name: Option<&str>,
    progress: &Progress,
) -> Result<Download, String> {
    let url = url(address)?;
    if let Some(name) = name {
        operations::valid_name(name)?;
    }
    let mut pending = Pending::new(folder)?;
    let headers = pending.directory.join("headers");
    let errors = pending.directory.join("errors");
    let mut child = Command::new("curl")
        .args([
            "--disable",
            "--globoff",
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--connect-timeout",
            "15",
            "--max-time",
            "300",
            "--dump-header",
        ])
        .arg(&headers)
        .arg("--url")
        .arg(url.as_str())
        .stdin(Stdio::null())
        .stdout(fs::File::create(pending.directory.join("data")).map_err(|e| e.to_string())?)
        .stderr(fs::File::create(&errors).map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    let status = loop {
        if progress.cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Download cancelled".into());
        }
        let done = fs::metadata(pending.directory.join("data")).map_or(0, |m| m.len());
        progress.done.store(done, Ordering::Relaxed);
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    };
    if !status.success() {
        return Err(fs::read_to_string(errors).unwrap_or_else(|_| "Download failed".into()));
    }
    let headers = fs::read_to_string(headers).map_err(|e| e.to_string())?;
    let headers = headers
        .split("\r\n\r\n")
        .filter(|part| part.starts_with("HTTP/"))
        .last()
        .unwrap_or("");
    let inferred = response_name(headers).or_else(|| path_name(&url));
    if let Some(name) = name.or(inferred.as_deref()) {
        return pending.save(name).map(Download::Saved);
    }
    let mime = header(headers, "content-type")
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("");
    let extension =
        mime_guess::get_mime_extensions_str(mime).and_then(|extensions| extensions.first());
    if let Some(extension) = extension {
        pending.suggested = format!("download.{extension}");
    }
    Ok(Download::Name(pending))
}

fn header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.trim())
}

fn decoded_name(value: &str) -> Option<String> {
    let path = Url::parse(&format!("file:///{value}"))
        .ok()?
        .to_file_path()
        .ok()?;
    let name = path.strip_prefix("/").ok()?.to_str()?;
    operations::valid_name(name).ok().map(str::to_owned)
}

fn response_name(headers: &str) -> Option<String> {
    let disposition = header(headers, "content-disposition")?;
    let parameters: Vec<_> = disposition
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .collect();
    let encoded = parameters
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("filename*"))
        .and_then(|(_, value)| {
            value
                .trim_matches('"')
                .strip_prefix("UTF-8''")
                .or_else(|| value.strip_prefix("utf-8''"))
        })
        .and_then(decoded_name);
    encoded.or_else(|| {
        parameters
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("filename"))
            .and_then(|(_, value)| operations::valid_name(value.trim_matches('"')).ok())
            .map(str::to_owned)
    })
}

fn path_name(url: &Url) -> Option<String> {
    let name = decoded_name(url.path_segments()?.next_back()?)?;
    Path::new(&name).extension().filter(|ext| !ext.is_empty())?;
    Some(name)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::filesystem::tests::Fixture;

    #[test]
    fn bytes_preserve_existing_files_and_reject_escaping_names() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("clipboard.png"), "original").unwrap();
        let path = bytes(&fixture.0, "clipboard.png", b"image").unwrap();
        assert_eq!(path, fixture.0.join("clipboard (copy).png"));
        assert_eq!(fs::read(path).unwrap(), b"image");
        assert_eq!(
            fs::read(fixture.0.join("clipboard.png")).unwrap(),
            b"original"
        );
        assert!(bytes(&fixture.0, "../escape", b"x").is_err());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
    }

    pub(crate) fn serve(response: impl Into<String>) -> (String, std::thread::JoinHandle<()>) {
        use std::io::Read;
        let response = response.into();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            loop {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut request = [0; 4096];
                    assert!(stream.read(&mut request).unwrap() > 0);
                    stream.write_all(response.as_bytes()).unwrap();
                    break;
                }
                assert!(
                    started.elapsed() < Duration::from_secs(60),
                    "no HTTP request arrived"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        (address, server)
    }

    #[test]
    fn downloads_publish_named_responses_and_prompt_for_unnamed_ones() {
        let fixture = Fixture::new();
        let (url, server) = serve(
            "HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=photo.png\r\nContent-Length: 3\r\n\r\nPNG",
        );
        let Download::Saved(path) = download(&fixture.0, &url, None, &Progress::default()).unwrap()
        else {
            panic!("missing name");
        };
        server.join().unwrap();
        assert_eq!(path, fixture.0.join("photo.png"));
        assert_eq!(fs::read(path).unwrap(), b"PNG");
        let (url, server) =
            serve("HTTP/1.1 200 OK\r\nContent-Type: image/webp\r\nContent-Length: 4\r\n\r\nWEBP");
        let Download::Name(pending) =
            download(&fixture.0, &url, None, &Progress::default()).unwrap()
        else {
            panic!("needs Save as");
        };
        server.join().unwrap();
        assert_eq!(pending.suggested, "download.webp");
        assert!(pending.save("../escape").is_err());
        let path = pending.save("my image.webp").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"WEBP");
        drop(pending);
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
    }

    #[test]
    fn explicit_names_conflicts_http_errors_and_cancel_leave_no_partial_files() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("saved.txt"), "original").unwrap();
        let (url, server) = serve("HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nnew");
        let Download::Saved(path) = download(
            &fixture.0,
            &format!("{url}/wrong.png"),
            Some("saved.txt"),
            &Progress::default(),
        )
        .unwrap() else {
            panic!("explicit name");
        };
        server.join().unwrap();
        assert_eq!(path, fixture.0.join("saved (copy).txt"));
        assert_eq!(fs::read(fixture.0.join("saved.txt")).unwrap(), b"original");
        let (url, server) = serve("HTTP/1.1 404 Not Found\r\nContent-Length: 3\r\n\r\nbad");
        assert!(download(&fixture.0, &url, None, &Progress::default()).is_err());
        server.join().unwrap();
        let cancelled = Progress::default();
        cancelled.cancel.store(true, Ordering::Relaxed);
        assert!(download(&fixture.0, "http://127.0.0.1:1", None, &cancelled).is_err());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 2);
    }

    #[test]
    fn redirects_use_final_headers_and_reject_local_file_protocols() {
        let fixture = Fixture::new();
        let (target, target_server) = serve(
            "HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=final.txt\r\nContent-Length: 3\r\n\r\nend",
        );
        // Each fixture runs once and the redirect header needs the ephemeral target port.
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Disposition: attachment; filename=wrong.txt\r\nContent-Length: 0\r\n\r\n"
        );
        let (url, server) = serve(response);
        let Download::Saved(path) = download(&fixture.0, &url, None, &Progress::default()).unwrap()
        else {
            panic!("redirect name");
        };
        server.join().unwrap();
        target_server.join().unwrap();
        assert_eq!(path, fixture.0.join("final.txt"));
        let (url, server) = serve(
            "HTTP/1.1 302 Found\r\nLocation: file:///etc/passwd\r\nContent-Length: 0\r\n\r\n",
        );
        assert!(download(&fixture.0, &url, None, &Progress::default()).is_err());
        server.join().unwrap();
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 1);
    }

    #[test]
    fn names_and_protocols_are_validated() {
        assert_eq!(
            response_name("Content-Disposition: attachment; filename=\"photo.jpg\""),
            Some("photo.jpg".into())
        );
        assert_eq!(
            response_name("content-disposition: attachment; filename*=UTF-8''hello%20world.png"),
            Some("hello world.png".into())
        );
        assert_eq!(
            response_name("Content-Disposition: attachment; filename=\"../escape\""),
            None
        );
        assert_eq!(
            response_name("Content-Disposition: attachment; filename*=UTF-8''..%2Fescape"),
            None
        );
        assert_eq!(
            path_name(&url("https://example.org/a%20b.png?q=x").unwrap()),
            Some("a b.png".into())
        );
        assert_eq!(
            path_name(&url("https://example.org/images?q=x").unwrap()),
            None
        );
        assert!(url("file:///etc/passwd").is_err());
    }
}
