//! Example: import annotations from a locally saved Pinterest/Cosmos scrape manifest.
//! The scraper itself is intentionally independent of Explorer and may use any Rust HTTP library.
use serde::{Deserialize, Serialize};
use std::{fs, io::{self, Read}, path::PathBuf};

#[derive(Deserialize)]
struct Request {
    api_version: u32,
    action: String,
    directory: PathBuf,
    arguments: String,
}

#[derive(Deserialize)]
struct Collection { items: Vec<Item> }
#[derive(Deserialize, Serialize)]
struct Item {
    file: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    creator: Option<String>,
}

#[derive(Serialize)]
struct Response {
    message: String,
    refresh: bool,
    updates: Vec<Item>,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Request = serde_json::from_str(&input)?;
    if request.api_version != 1 || request.action != "run" {
        return Err("Unsupported Explorer extension API".into());
    }
    if request.arguments.trim().is_empty() {
        return Err("Usage: :script import_annotations manifest.json".into());
    }
    let manifest = PathBuf::from(request.arguments.trim());
    let manifest = if manifest.is_absolute() { manifest } else { request.directory.join(manifest) };
    let collection: Collection = serde_json::from_slice(&fs::read(manifest)?)?;
    let count = collection.items.len();
    let response = Response {
        message: format!("Imported metadata for {count} image references"),
        refresh: true,
        updates: collection.items,
    };
    println!("{}", serde_json::to_string(&response)?);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
