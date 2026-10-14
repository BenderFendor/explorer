//! `org.freedesktop.FileManager1`: what browsers and other apps call for "Show in folder".
//! Started by D-Bus activation (`--dbus-service`); each call opens an explorer window.
//! The service exits after a quiet minute and D-Bus starts it again on demand.

use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const NAME: &str = "org.freedesktop.FileManager1";
const PATH: &str = "/org/freedesktop/FileManager1";
const IDLE: Duration = Duration::from_secs(60);

struct FileManager {
    last_call: Arc<Mutex<Instant>>,
}

fn paths(uris: &[String]) -> Vec<PathBuf> {
    uris.iter()
        .filter_map(|uri| url::Url::parse(uri).ok()?.to_file_path().ok())
        .collect()
}

fn open(arguments: &[std::ffi::OsString]) {
    if let Ok(program) = std::env::current_exe() {
        let _ = Command::new(program).args(arguments).spawn();
    }
}

impl FileManager {
    fn touch(&self) {
        if let Ok(mut last) = self.last_call.lock() {
            *last = Instant::now();
        }
    }
}

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl FileManager {
    fn show_folders(&self, uris: Vec<String>, _startup_id: &str) {
        self.touch();
        for folder in paths(&uris) {
            open(&["--".into(), folder.into()]);
        }
    }

    /// Opens each item's folder with the item under the cursor.
    fn show_items(&self, uris: Vec<String>, _startup_id: &str) {
        self.touch();
        for item in paths(&uris) {
            let mut select = std::ffi::OsString::from("--select=");
            select.push(&item);
            open(&[select]);
        }
    }

    /// There is no properties dialog; the folder opens with the item selected.
    fn show_item_properties(&self, uris: Vec<String>, startup_id: &str) {
        self.show_items(uris, startup_id);
    }
}

pub fn serve() -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let last_call = Arc::new(Mutex::new(Instant::now()));
        let manager = FileManager {
            last_call: last_call.clone(),
        };
        let _connection = zbus::connection::Builder::session()
            .and_then(|builder| builder.name(NAME))
            .and_then(|builder| builder.serve_at(PATH, manager))
            .map_err(|error| error.to_string())?
            .build()
            .await
            .map_err(|error| format!("{NAME}: {error}"))?;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let quiet = last_call.lock().map(|last| last.elapsed()).unwrap_or(IDLE);
            if quiet >= IDLE {
                return Ok(());
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_file_uris_open() {
        let uris = [
            "file:///mnt/Big%20storage/a.txt".into(),
            "https://x.org".into(),
        ];
        assert_eq!(paths(&uris), [PathBuf::from("/mnt/Big storage/a.txt")]);
    }
}
