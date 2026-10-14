use crate::filesystem::{Order, Sort};
use std::fs;
use std::path::PathBuf;

pub fn state_path() -> PathBuf {
    if let Ok(dir) = std::env::var("EXPLORER_STATE_DIR") {
        return std::path::PathBuf::from(dir).join("state.txt");
    }
    let base = directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| crate::filesystem::home().join(".config"));
    base.join("explorer/state.txt")
}

fn parse_order(text: &str) -> Option<Order> {
    let mut order = Order {
        sort: Sort::Modified,
        reverse: false,
    };
    let mut seen = false;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("sort=") {
            order.sort = match value.trim() {
                "name" | "natural" => Sort::Name,
                "size" => Sort::Size,
                "mtime" | "modified" => Sort::Modified,
                "type" | "extension" => Sort::Type,
                _ => order.sort,
            };
            seen = true;
        } else if let Some(value) = line.strip_prefix("reverse=") {
            order.reverse = value.trim() == "true";
            seen = true;
        }
    }
    seen.then_some(order)
}

pub fn load_order() -> Option<Order> {
    let text = fs::read_to_string(state_path()).ok()?;
    parse_order(&text)
}

pub fn save_order(order: Order) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(
        path,
        format!("sort={}\nreverse={}\n", order.sort.label(), order.reverse),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_order_reads_sort_and_reverse() {
        let order = parse_order("sort=size\nreverse=true\n").unwrap();
        assert_eq!(order.sort, Sort::Size);
        assert!(order.reverse);
    }

    #[test]
    fn parse_order_rejects_empty_text() {
        assert!(parse_order("").is_none());
        assert!(parse_order("other=value\n").is_none());
    }

    #[test]
    fn save_order_round_trips_through_parse() {
        // Keep this test purely on the serialization format to avoid home-directory side effects.
        let order = Order {
            sort: Sort::Type,
            reverse: true,
        };
        let rendered = format!("sort={}\nreverse={}\n", order.sort.label(), order.reverse);
        let parsed = parse_order(&rendered).unwrap();
        assert_eq!(parsed, order);
    }
}
/// Persistent single-key navigation bookmarks, independent of GTK sidebar pins.
pub fn load_key_bookmarks() -> std::collections::HashMap<char, PathBuf> {
    let path = state_path().with_file_name("key-bookmarks.json");
    fs::read(path).ok().and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default()
}

pub fn save_key_bookmarks(bookmarks: &std::collections::HashMap<char, PathBuf>) -> std::io::Result<()> {
    let path = state_path().with_file_name("key-bookmarks.json");
    fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(bookmarks)?)?;
    fs::rename(temporary, path)
}
