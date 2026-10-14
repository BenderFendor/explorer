//! On-demand, bounded local navigation history. Never scans filesystems in the background.
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Visit {
    pub path: PathBuf,
    pub count: u32,
    pub last: u64,
    pub folder: bool,
}

fn history_path() -> PathBuf {
    let base = std::env::var_os("EXPLORER_STATE_DIR").map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.state_dir().unwrap_or(dirs.data_local_dir()).to_path_buf()))
        .unwrap_or_else(|| crate::filesystem::home().join(".local/state"));
    base.join("explorer/history.json")
}

pub fn load() -> Vec<Visit> {
    fs::read(history_path()).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default()
}

fn save(visits: &[Visit]) -> std::io::Result<()> {
    let target = history_path();
    fs::create_dir_all(target.parent().unwrap_or(Path::new(".")))?;
    let temp = target.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(visits)?)?;
    fs::rename(temp, target)
}

pub fn record(target: &Path, folder: bool) {
    if !target.exists() || std::env::var_os("EXPLORER_DISABLE_HISTORY").is_some() { return; }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut visits = load();
    if let Some(item) = visits.iter_mut().find(|item| item.path == target) {
        item.count = item.count.saturating_add(1);
        item.last = now;
        item.folder = folder;
    } else {
        visits.push(Visit { path: target.to_path_buf(), count: 1, last: now, folder });
    }
    visits.sort_by_key(|item| std::cmp::Reverse(item.last));
    visits.truncate(256);
    let _ = save(&visits);
}

pub fn recent(folder: bool, limit: usize) -> Vec<Visit> {
    let mut visits: Vec<_> = load().into_iter().filter(|item| item.folder == folder && item.path.exists()).collect();
    visits.sort_by_key(|item| std::cmp::Reverse(item.last));
    visits.truncate(limit);
    visits
}

pub fn frequent(limit: usize) -> Vec<Visit> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let mut visits: Vec<_> = load().into_iter().filter(|item| item.folder && item.path.is_dir()).collect();
    visits.sort_by(|a, b| {
        let score = |v: &Visit| (v.count as f64).ln_1p() / (1.0 + now.saturating_sub(v.last) as f64 / 604800.0);
        score(b).total_cmp(&score(a))
    });
    visits.truncate(limit);
    visits
}

pub fn clear() -> std::io::Result<()> {
    match fs::remove_file(history_path()) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visit_round_trip() {
        let value = Visit { path: PathBuf::from("/tmp/p"), count: 2, last: 30, folder: false };
        assert_eq!(serde_json::from_str::<Visit>(&serde_json::to_string(&value).unwrap()).unwrap(), value);
    }
}
