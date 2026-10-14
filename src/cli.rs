use crate::shell::Backend;
use std::{ffi::OsString, path::PathBuf, sync::OnceLock};

#[derive(Debug, Default)]
pub struct Options {
    pub directory: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub renderer: Option<Backend>,
    pub bench: bool,
    pub dbus_service: bool,
    /// Opens this item's folder with the item under the cursor.
    pub select: Option<PathBuf>,
    pub properties: Option<PathBuf>,
}

impl Options {
    /// The flag wins, then `EXPLORER_RENDERER`, then the GPU renderer.
    pub fn backend(&self) -> Result<Backend, String> {
        match (self.renderer, std::env::var("EXPLORER_RENDERER")) {
            (Some(backend), _) => Ok(backend),
            (None, Ok(value)) => backend(&value),
            (None, Err(_)) => Ok(Backend::Gpu),
        }
    }
}

fn backend(value: &str) -> Result<Backend, String> {
    match value {
        "cpu" => Ok(Backend::Cpu),
        "gpu" => Ok(Backend::Gpu),
        _ => Err(format!("Unknown renderer {value:?}; use cpu or gpu")),
    }
}

static OPTIONS: OnceLock<Options> = OnceLock::new();

pub fn options() -> &'static Options {
    OPTIONS.get_or_init(Options::default)
}

pub fn initialize() -> Result<bool, String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "--help" || arg == "-h")
    {
        println!(
            "Usage: explorer [DIRECTORY] [--renderer=cpu|gpu] [--screenshot[=FILE.png] | --bench]\n\n--renderer picks software (cpu) or graphics-card (gpu, default) drawing.\n           EXPLORER_RENDERER sets the same choice.\n--screenshot saves explorer-screenshot.png and exits, without desktop tools.\n--bench      scrolls the app off-screen and prints renderer cost as JSON.\n--select=PATH opens PATH's folder with PATH selected.\n--properties=PATH opens the actual file properties inspector.\n--dbus-service runs org.freedesktop.FileManager1 (\"Show in folder\")."
        );
        return Ok(false);
    }
    OPTIONS
        .set(parse(arguments)?)
        .map_err(|_| "Options already initialized")?;
    Ok(true)
}

fn parse(arguments: Vec<OsString>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut positional = false;
    for argument in arguments {
        if argument == "--" && !positional {
            positional = true;
            continue;
        }
        if !positional && argument.as_encoded_bytes().starts_with(b"-") {
            if argument == "--bench" || argument == "--dbus-service" {
                options.bench |= argument == "--bench";
                options.dbus_service |= argument == "--dbus-service";
                continue;
            }
            if let Some(path) = argument
                .to_str()
                .and_then(|text| text.strip_prefix("--select="))
            {
                options.select = Some(PathBuf::from(path));
                continue;
            }
            if let Some(path) = argument
                .to_str()
                .and_then(|text| text.strip_prefix("--properties="))
            {
                options.properties = Some(PathBuf::from(path));
                options.select = Some(PathBuf::from(path));
                continue;
            }
            match argument
                .to_str()
                .and_then(|text| text.strip_prefix("--renderer="))
            {
                Some(value) => options.renderer = Some(backend(value)?),
                None => options.screenshot = Some(screenshot_path(&argument)?),
            }
            continue;
        }
        if options.directory.replace(PathBuf::from(argument)).is_some() {
            return Err("Only one initial directory is supported".into());
        }
    }
    Ok(options)
}

fn screenshot_path(argument: &std::ffi::OsStr) -> Result<PathBuf, String> {
    if argument == "--screenshot" {
        return Ok(PathBuf::from("explorer-screenshot.png"));
    }
    let path = argument
        .to_str()
        .and_then(|value| value.strip_prefix("--screenshot="))
        .ok_or_else(|| format!("Unknown option: {}", argument.to_string_lossy()))?;
    if path.is_empty() {
        return Err("Screenshot output path is empty".into());
    }
    Ok(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn screenshot_does_not_become_the_initial_directory() {
        let options = parse(args(&["--screenshot", "Pictures"])).unwrap();
        assert_eq!(options.directory, Some(PathBuf::from("Pictures")));
        assert_eq!(
            options.screenshot,
            Some(PathBuf::from("explorer-screenshot.png"))
        );
        let options = parse(args(&["Pictures", "--screenshot=/tmp/capture.png"])).unwrap();
        assert_eq!(options.screenshot, Some(PathBuf::from("/tmp/capture.png")));
        assert!(parse(args(&["--screenshot="])).is_err());
        assert!(parse(args(&["--unknown"])).is_err());
        assert_eq!(
            parse(args(&["--renderer=gpu"])).unwrap().renderer,
            Some(Backend::Gpu)
        );
        assert!(parse(args(&["--renderer=vulkan"])).is_err());
        let select = parse(args(&["--select=/mnt/a b/c.txt"])).unwrap();
        assert_eq!(select.select, Some(PathBuf::from("/mnt/a b/c.txt")));
        assert!(parse(args(&["one", "two"])).is_err());
        assert_eq!(
            parse(args(&["--", "--screenshot"])).unwrap().directory,
            Some(PathBuf::from("--screenshot"))
        );
    }
}
