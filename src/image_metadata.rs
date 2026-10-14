//! Portable, file-first gallery annotations, stored only when a user writes them.
//! Ordinary image folders need no manifest, database or import step.
use serde::{Deserialize, Serialize};
use std::{
    fmt::Write as _,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ImageMetadata {
    pub title: String,
    pub caption: String,
    pub source: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub creator: String,
}

pub fn metadata_path(image: &Path) -> PathBuf {
    let mut hex = String::new();
    if let Some(name) = image.file_name() {
        for byte in name.as_bytes() {
            let _ = write!(&mut hex, "{byte:02x}");
        }
    }
    image
        .parent()
        .unwrap_or(Path::new("."))
        .join(".explorer/metadata")
        .join(format!("{hex}.json"))
}

pub fn load(image: &Path) -> ImageMetadata {
    fs::read(metadata_path(image))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn atomic_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

pub fn save(image: &Path, metadata: &ImageMetadata) -> io::Result<()> {
    if !image.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Image does not exist",
        ));
    }
    atomic_json(&metadata_path(image), metadata)
}

pub fn update(image: &Path, field: &str, value: &str) -> io::Result<()> {
    let mut metadata = load(image);
    match field {
        "title" => metadata.title = value.to_owned(),
        "caption" => metadata.caption = value.to_owned(),
        "source" => metadata.source = value.to_owned(),
        "notes" => metadata.notes = value.to_owned(),
        "tags" => {
            metadata.tags = value
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        }
        "creator" => metadata.creator = value.to_owned(),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unknown metadata field",
            ));
        }
    }
    save(image, &metadata)
}

/// Copies/moves should preserve companion annotation files, without rewriting original pixels.
pub fn copy_sidecar(from: &Path, to: &Path) -> io::Result<()> {
    let source = metadata_path(from);
    if !source.exists() {
        return Ok(());
    }
    let target = metadata_path(to);
    if target.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Destination metadata exists",
        ));
    }
    fs::create_dir_all(target.parent().unwrap_or(Path::new(".")))?;
    fs::copy(source, target)?;
    Ok(())
}

pub fn move_sidecar(from: &Path, to: &Path) -> io::Result<()> {
    let source = metadata_path(from);
    if !source.exists() {
        return Ok(());
    }
    let target = metadata_path(to);
    if target.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Destination metadata exists",
        ));
    }
    fs::create_dir_all(target.parent().unwrap_or(Path::new(".")))?;
    fs::rename(source, target)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[allow(dead_code)]
pub struct Board {
    pub version: u32,
    pub cards: Vec<Card>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
#[allow(dead_code)]
pub struct Card {
    pub file: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub note: String,
}

#[allow(dead_code)]
pub fn load_board(folder: &Path) -> Board {
    fs::read(folder.join(".explorer/board.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn save_board(folder: &Path, board: &Board) -> io::Result<()> {
    for card in &board.cards {
        if Path::new(&card.file).components().count() != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Board cards must be in this folder",
            ));
        }
    }
    atomic_json(&folder.join(".explorer/board.json"), board)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_names_handle_spaces_and_utf8() {
        let image = Path::new("/tmp/My Boards/a b.png");
        assert!(metadata_path(image).ends_with(".explorer/metadata/6120622e706e67.json"));
    }
    #[test]
    fn board_roundtrip() {
        let board = Board {
            version: 1,
            cards: vec![Card {
                file: "a.jpg".into(),
                x: 4.0,
                ..Default::default()
            }],
        };
        assert_eq!(
            serde_json::from_str::<Board>(&serde_json::to_string(&board).unwrap()).unwrap(),
            board
        );
    }
}

/// Read common embedded XMP fields via the optional Exiv2 system utility.
/// Existing sidecars take precedence; this never requires the utility to start Explorer.
pub fn read_xmp(image: &Path) -> io::Result<ImageMetadata> {
    let output=std::process::Command::new("exiv2").arg("-Pkv").arg(image).output()?;
    if !output.status.success() {return Err(io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()));}
    let mut data=ImageMetadata::default();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        // Exiv2 -Pkv lines: Key  Type  Count  Value.
        let mut fields=line.split_whitespace();
        let Some(key)=fields.next() else {continue;};
        let _type=fields.next();let _count=fields.next();
        let value=fields.collect::<Vec<_>>().join(" ");
        let value=value.strip_prefix("lang=\"x-default\" ").unwrap_or(&value).to_owned();
        match key {
            "Xmp.dc.title" => data.title=value,
            "Xmp.dc.description" => data.caption=value,
            "Xmp.dc.creator" => data.creator=value,
            "Xmp.dc.source" => data.source=value,
            "Xmp.dc.subject" => {if !data.tags.contains(&value){data.tags.push(value);}},
            _ => {}
        }
    }
    Ok(data)
}

/// Merge EXIF/IPTC/XMP without recompression using Exiv2. Writes to a sibling
/// temporary copy then atomically replaces the original only on success.
/// Sidecars remain the canonical store for notes and nonstandard fields.
pub fn write_xmp(image:&Path)->io::Result<()> {
    if !image.is_file() || image.is_symlink() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,"Select a regular image"));
    }
    let metadata=load(image);
    // Exiv2 selects write format from extension: use same original suffix.
    let tmp_name=format!(".explorer-xmp-{}.{}",std::process::id(),image.extension().unwrap_or_default().to_string_lossy());
    let temp=image.with_file_name(tmp_name);
    fs::copy(image,&temp)?;
    let commands=[
        ("Xmp.dc.title",&metadata.title),
        ("Xmp.dc.description",&metadata.caption),
        ("Xmp.dc.creator",&metadata.creator),
        ("Xmp.dc.source",&metadata.source),
    ];
    let mut command=std::process::Command::new("exiv2");
    for (key,value) in commands {
        if !value.is_empty() {
            // -M passes each metadatum modification as one argument (no shell).
            command.arg("-M").arg(format!("set {key} {}",value.replace('\n'," ")));
        }
    }
    for tag in &metadata.tags {
        if !tag.is_empty(){command.arg("-M").arg(format!("add Xmp.dc.subject XmpBag {}",tag.replace('\n'," ")));}
    }
    let result=command.arg(&temp).output();
    match result {
        Ok(output) if output.status.success() => {
            // Retain the original mode and mtime to avoid unwanted permission changes.
            fs::set_permissions(&temp,fs::metadata(image)?.permissions())?;
            fs::rename(&temp,image)
        }
        Ok(output) => {let _=fs::remove_file(&temp);Err(io::Error::other(String::from_utf8_lossy(&output.stderr).into_owned()))}
        Err(e) => {let _=fs::remove_file(&temp);Err(e)}
    }
}
