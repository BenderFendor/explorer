use base64::{Engine, engine::general_purpose::STANDARD};
use notify::{EventKind, RecursiveMode, Watcher};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Theme {
    #[default]
    Matugen,
    Light,
    Dark,
}

impl Theme {
    pub fn next(self) -> Self {
        match self {
            Self::Matugen => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::Matugen,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Matugen => "Matugen",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
    pub fn class(self) -> &'static str {
        match self {
            Self::Matugen => "matugen",
            Self::Light => "",
            Self::Dark => "dark",
        }
    }
}

pub fn palette_path() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| crate::filesystem::home().join(".config"))
        .join("matugen/colors.css")
}

pub fn read_palette() -> Option<String> {
    palette_css(&fs::read_to_string(palette_path()).ok()?)
}

pub fn palette_css(text: &str) -> Option<String> {
    let colors: HashMap<_, _> = text
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            if words.next()? != "@define-color" {
                return None;
            }
            let name = words.next()?;
            let value = words.next()?.trim_end_matches(';');
            let hex = value.strip_prefix('#')?;
            (matches!(hex.len(), 3 | 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .then_some((name, value))
        })
        .collect();
    let mut css = String::from(".app.matugen {");
    for (token, source) in [
        ("surface", "surface"),
        ("raised", "surface_container_high"),
        ("text", "on_surface"),
        ("muted", "on_surface_variant"),
        ("line", "outline_variant"),
        ("hover", "surface_container"),
        ("active", "surface_container_highest"),
        ("accent", "primary"),
        ("folder-back", "primary_fixed_dim"),
        ("folder-front", "primary_fixed"),
        ("paper", "surface_container_low"),
        ("error", "error"),
        ("error-bg", "error_container"),
    ] {
        let color = colors.get(source).or_else(|| colors.get("primary"))?;
        css.push_str(&format!("--{token}:{color};"));
    }
    css.push('}');
    Some(css)
}

pub async fn watch_palette(mut update: impl FnMut(String)) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let path = palette_path();
    let observed = path.clone();
    let Ok(mut watcher) =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else {
                return;
            };
            if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_))
                && event.paths.contains(&observed)
                && let Some(css) = read_palette()
            {
                let _ = tx.send(css);
            }
        })
    else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if watcher.watch(parent, RecursiveMode::NonRecursive).is_err() {
        return;
    }
    while let Some(css) = rx.recv().await {
        update(css);
    }
}

pub fn font_stylesheet() -> String {
    let mut css = String::new();
    for (family, weight, bytes) in [
        (
            "General Sans",
            400,
            include_bytes!("../assets/fonts/GeneralSans-Regular.ttf").as_slice(),
        ),
        (
            "General Sans",
            500,
            include_bytes!("../assets/fonts/GeneralSans-Medium.ttf").as_slice(),
        ),
        (
            "Satoshi",
            400,
            include_bytes!("../assets/fonts/Satoshi-Regular.ttf").as_slice(),
        ),
        (
            "Satoshi",
            500,
            include_bytes!("../assets/fonts/Satoshi-Medium.ttf").as_slice(),
        ),
    ] {
        css.push_str(&format!("@font-face {{font-family:'{family}';font-weight:{weight};src:url('data:font/ttf;base64,{}') format(truetype);}}", STANDARD.encode(bytes)));
    }
    format!("data:text/css;base64,{}", STANDARD.encode(css))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gtk_palette_becomes_safe_css_tokens() {
        let css =
            palette_css("@define-color primary #ABCDEF;\n@define-color surface #123456;").unwrap();
        assert!(css.contains("--surface:#123456;"));
        assert!(css.contains("--folder-back:#ABCDEF;"));
        assert!(palette_css("@define-color primary url(secret);").is_none());
        assert!(palette_css("@define-color primary #bad<script>;").is_none());
        assert!(palette_css("").is_none());
    }
}
