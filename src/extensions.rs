//! Explicitly invoked local Rust executables; extensions are not loaded into Explorer's process.
//! The JSON protocol is versioned and independent of UI, indexing, or download implementation.
//! Executables have the user's OS permissions: only install trusted extensions.
use serde::{Serialize, Deserialize};
use std::{fs, path::{Path,PathBuf}, process::{Command,Stdio}, io::Write};

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub api_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub executable: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct Request {
    pub api_version: u32,
    pub action: &'static str,
    pub directory: PathBuf,
    pub selected_paths: Vec<PathBuf>,
    pub arguments: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Response {
    pub message: String,
    pub refresh: bool,
    pub updates: Vec<Update>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Update {
    pub path: PathBuf,
    pub title: Option<String>,
    pub caption: Option<String>,
    pub source: Option<String>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub creator: Option<String>,
}

pub fn directory() -> PathBuf {
    directories::BaseDirs::new().map(|d| d.config_dir().join("explorer/extensions"))
        .unwrap_or_else(|| crate::filesystem::home().join(".config/explorer/extensions"))
}

pub fn list() -> Vec<Manifest> {
    let Ok(entries) = fs::read_dir(directory()) else { return Vec::new(); };
    let mut found: Vec<_> = entries.flatten().filter_map(|entry| {
        if entry.path().extension().is_none_or(|ext| ext != "json") { return None; }
        let bytes = fs::read(entry.path()).ok()?;
        let mut manifest: Manifest = serde_json::from_slice(&bytes).ok()?;
        if manifest.api_version != 1 || manifest.name.is_empty() { return None; }
        if !manifest.executable.is_absolute() {
            manifest.executable = entry.path().parent()?.join(&manifest.executable);
        }
        Some(manifest)
    }).collect();
    found.sort_by(|a,b| a.name.cmp(&b.name));
    found
}

pub fn run(name: &str, arguments: String, directory: PathBuf, selected_paths: Vec<PathBuf>) -> Result<String,String> {
    let manifest = list().into_iter().find(|ext| ext.name == name)
        .ok_or_else(|| format!("Unknown extension {name}. Use :scripts to list installed extensions"))?;
    if !manifest.executable.is_file() {
        return Err(format!("Extension executable missing: {}",manifest.executable.display()));
    }
    let request = Request { api_version: 1, action: "run", directory: directory.clone(), selected_paths, arguments };
    let input = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    let mut child = Command::new(&manifest.executable)
        .current_dir(&directory)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("{}: {e}", manifest.name))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&input).map_err(|e| e.to_string())?;
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("{} exited {}: {}", manifest.name, output.status,
            String::from_utf8_lossy(&output.stderr).chars().take(500).collect::<String>()));
    }
    if output.stdout.len() > 1024 * 1024 { return Err("Extension output exceeded 1 MiB".into()); }
    let response: Response = serde_json::from_slice(&output.stdout).map_err(|e| format!("Extension response invalid: {e}"))?;
    let root = fs::canonicalize(&directory).map_err(|e| e.to_string())?;
    for update in response.updates {
        let target = if update.path.is_absolute() { update.path } else { directory.join(update.path) };
        let canonical = fs::canonicalize(&target).map_err(|e| e.to_string())?;
        if !canonical.starts_with(&root) || !canonical.is_file() {
            return Err("Extension attempted metadata update outside this folder".into());
        }
        let mut metadata = crate::image_metadata::load(&canonical);
        if let Some(v) = update.title { metadata.title = v; }
        if let Some(v) = update.caption { metadata.caption = v; }
        if let Some(v) = update.source { metadata.source = v; }
        if let Some(v) = update.notes { metadata.notes = v; }
        if let Some(v) = update.tags { metadata.tags = v; }
        if let Some(v) = update.creator { metadata.creator = v; }
        crate::image_metadata::save(&canonical, &metadata).map_err(|e| e.to_string())?;
    }
    Ok(if response.message.is_empty() {
        format!("Extension {name} completed")
    } else { response.message })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_roundtrip() {
        let response: Response = serde_json::from_str(r#"{"message":"ok","updates":[]}"#).unwrap();
        assert_eq!(response.message, "ok");
        assert!(!response.refresh);
    }
}
