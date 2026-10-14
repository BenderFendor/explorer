//! Example extension: group common file formats into sibling folders.
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::{self, Read}, path::{Path, PathBuf}};

#[derive(Deserialize)]
struct Request {
    api_version: u32,
    action: String,
    directory: PathBuf,
    selected_paths: Vec<PathBuf>,
    arguments: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum FileOperation {
    CreateFolder { name: String },
    Move { path: PathBuf, folder: PathBuf },
}

#[derive(Serialize)]
struct Response {
    message: String,
    refresh: bool,
    operations: Vec<FileOperation>,
    updates: Vec<serde_json::Value>,
}

fn category(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "avif" => Some("Images"),
        "mp4" | "mov" | "mkv" | "webm" => Some("Videos"),
        "pdf" | "epub" | "md" | "txt" | "docx" => Some("Documents"),
        _ => None,
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    if request.api_version != 1 || request.action != "run" {
        return Err("Unsupported Explorer extension protocol".into());
    }
    let all = request.arguments.trim() == "all";
    let items: Vec<PathBuf> = if all {
        fs::read_dir(&request.directory)?.filter_map(Result::ok).map(|x| x.path()).collect()
    } else {
        request.selected_paths
    };
    let mut new_dirs = HashSet::new();
    let mut operations = Vec::new();
    let mut count = 0;
    for path in items {
        if !path.is_file() || path.parent() != Some(request.directory.as_path()) {
            continue;
        }
        let Some(folder) = category(&path) else { continue; };
        let target = request.directory.join(folder);
        if !target.exists() && new_dirs.insert(folder) {
            operations.push(FileOperation::CreateFolder { name: folder.to_string() });
        }
        operations.push(FileOperation::Move { path, folder: target });
        count += 1;
    }
    println!("{}", serde_json::to_string(&Response {
        message: format!("Organized {count} files"),
        refresh: true,
        operations, updates: vec![],
    })?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
