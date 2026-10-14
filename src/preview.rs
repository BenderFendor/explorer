use std::{fs::File, io::Read, path::Path};

const BYTES: u64 = 16 * 1024;
const LINES: usize = 120;

/// Returns the first lines of a text file, or None for binary or unreadable files.
pub fn text(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.contains(&0) {
        return None;
    }
    // A read limit can split a multi-byte character; keep the valid prefix.
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(error) if bytes.len() - error.valid_up_to() < 4 => {
            std::str::from_utf8(&bytes[..error.valid_up_to()]).ok()?
        }
        Err(_) => return None,
    };
    Some(
        text.lines()
            .take(LINES)
            .map(|line| line.replace('\t', "    "))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::tests::Fixture;

    #[test]
    fn previews_text_and_rejects_binary() {
        let fixture = Fixture::new();
        let note = fixture.0.join("note.md");
        std::fs::write(&note, "# Title\n\tindented\n").unwrap();
        assert_eq!(text(&note).unwrap(), "# Title\n    indented");
        let binary = fixture.0.join("data.bin");
        std::fs::write(&binary, [0x89, b'P', 0, 1]).unwrap();
        assert!(text(&binary).is_none());
        let long = fixture.0.join("long.txt");
        let mut content = "é".repeat(BYTES as usize / 2 + 3);
        content.insert(0, 'a');
        std::fs::write(&long, content).unwrap();
        assert!(
            text(&long).is_some(),
            "a character split by the limit is dropped"
        );
        assert!(text(&fixture.0.join("missing")).is_none());
    }
}
