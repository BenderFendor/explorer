//! Key bindings: ranger sequences for plain keys, Thunar chords for modified keys,
//! and the `:` console vocabulary. Everything here is pure so it can be tested.

use super::viewport::View;
use crate::filesystem::Sort;
use dioxus_native::prelude::{Key, Modifiers};

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Move(isize),
    /// Arrow left/right: neighbouring item in the grid, parent/open in rows.
    Step(isize),
    MarkMove(isize),
    Top,
    Bottom,
    Page(f64),
    Parent,
    Open,
    Left,
    Right,
    ToggleGallery,
    Back,
    Forward,
    Mark,
    InvertMarks,
    ClearMarks,
    SelectAll,
    Yank,
    Cut,
    Paste,
    Trash,
    Delete,
    Rename,
    Console(&'static str),
    Filter,
    ToggleHidden,
    SortBy(Sort),
    Reverse,
    Refresh,
    Home,
    Root,
    Trashcan,
    NewTab,
    NextTab,
    PreviousTab,
    CloseTab,
    NewWindow,
    Terminal,
    TogglePreview,
    ToggleSidebar,
    SetView(View),
    /// +1 zoom in, -1 zoom out, 0 reset.
    Zoom(i32),
    Help,
    EditPath,
    SetBookmark(char),
    Bookmark(char),
    Escape,
    // Console-only commands with arguments.
    Cd(String),
    MakeFolder(String),
    MakeFile(String),
    RenameTo(String),
    OpenWith(String),
    Download(String),
    Restore,
    EmptyTrash,
    PinCurrent,
    PinSelection,
    ClearHistory,
    ListScripts,
    RunScript(String),
    EditMetadata(&'static str, String),
}

#[derive(Debug, PartialEq)]
pub enum Resolution {
    Run(Command),
    Pending,
    Unknown,
}

type Binding = (&'static str, fn() -> Command);

const SEQUENCES: &[Binding] = &[
    ("j", || Command::Move(1)),
    ("k", || Command::Move(-1)),
    ("h", || Command::Left),
    ("l", || Command::Right),
    ("gg", || Command::Top),
    ("G", || Command::Bottom),
    ("H", || Command::Back),
    ("L", || Command::Forward),
    (" ", || Command::Mark),
    ("v", || Command::InvertMarks),
    ("uv", || Command::ClearMarks),
    ("yy", || Command::Yank),
    ("dd", || Command::Cut),
    ("pp", || Command::Paste),
    ("dT", || Command::Trash),
    ("dD", || Command::Delete),
    ("cw", || Command::Rename),
    ("A", || Command::Rename),
    (":", || Command::Console("")),
    ("r", || Command::Console("open_with ")),
    ("/", || Command::Filter),
    ("f", || Command::Filter),
    ("zh", || Command::ToggleHidden),
    ("zg", || Command::ToggleGallery),
    (".", || Command::ToggleHidden),
    ("on", || Command::SortBy(Sort::Name)),
    ("ob", || Command::SortBy(Sort::Name)),
    ("os", || Command::SortBy(Sort::Size)),
    ("om", || Command::SortBy(Sort::Modified)),
    ("ot", || Command::SortBy(Sort::Type)),
    ("or", || Command::Reverse),
    ("gh", || Command::Home),
    ("~", || Command::Home),
    ("g/", || Command::Root),
    ("gx", || Command::Trashcan),
    ("gn", || Command::NewTab),
    ("gt", || Command::NextTab),
    ("gT", || Command::PreviousTab),
    ("q", || Command::CloseTab),
    ("S", || Command::Terminal),
    ("i", || Command::TogglePreview),
    ("zs", || Command::ToggleSidebar),
    ("?", || Command::Help),
    ("+", || Command::Zoom(1)),
    ("=", || Command::Zoom(1)),
    ("-", || Command::Zoom(-1)),
];

/// Resolves a typed key sequence. `m` and `'` take any following character.
pub fn resolve(sequence: &str) -> Resolution {
    let mut chars = sequence.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some('m'), None, _) | (Some('\''), None, _) => return Resolution::Pending,
        (Some('m'), Some(key), None) => return Resolution::Run(Command::SetBookmark(key)),
        (Some('\''), Some(key), None) => return Resolution::Run(Command::Bookmark(key)),
        _ => {}
    }
    if let Some((_, command)) = SEQUENCES.iter().find(|(keys, _)| *keys == sequence) {
        return Resolution::Run(command());
    }
    if SEQUENCES.iter().any(|(keys, _)| keys.starts_with(sequence)) {
        Resolution::Pending
    } else {
        Resolution::Unknown
    }
}

/// Named keys: key, whether Shift is held, command.
const NAMED: &[(Key, bool, Command)] = &[
    (Key::ArrowDown, true, Command::MarkMove(1)),
    (Key::ArrowUp, true, Command::MarkMove(-1)),
    (Key::Delete, true, Command::Delete),
    (Key::ArrowDown, false, Command::Move(1)),
    (Key::ArrowUp, false, Command::Move(-1)),
    (Key::ArrowLeft, false, Command::Step(-1)),
    (Key::Backspace, false, Command::Parent),
    (Key::ArrowRight, false, Command::Step(1)),
    (Key::Enter, false, Command::Open),
    (Key::PageDown, false, Command::Page(1.0)),
    (Key::PageUp, false, Command::Page(-1.0)),
    (Key::Home, false, Command::Top),
    (Key::End, false, Command::Bottom),
    (Key::Delete, false, Command::Trash),
    (Key::F2, false, Command::Rename),
    (Key::F4, false, Command::Terminal),
    (Key::F5, false, Command::Refresh),
    (Key::F9, false, Command::ToggleSidebar),
    (Key::Escape, false, Command::Escape),
];

const ALT: &[(Key, Command)] = &[
    (Key::ArrowLeft, Command::Back),
    (Key::ArrowRight, Command::Forward),
    (Key::ArrowUp, Command::Parent),
    (Key::Home, Command::Home),
];

/// Ctrl chords: lowercase key, required Shift state (None accepts both), command.
const CONTROL: &[(&str, Option<bool>, Command)] = &[
    ("n", Some(true), Command::Console("mkdir ")),
    ("n", Some(false), Command::NewWindow),
    ("d", None, Command::Page(0.5)),
    ("u", None, Command::Page(-0.5)),
    ("f", None, Command::Page(1.0)),
    ("b", None, Command::Page(-1.0)),
    ("a", None, Command::SelectAll),
    ("c", None, Command::Yank),
    ("x", None, Command::Cut),
    ("v", None, Command::Paste),
    ("h", None, Command::ToggleHidden),
    ("l", None, Command::EditPath),
    ("r", None, Command::Refresh),
    ("t", None, Command::NewTab),
    ("w", None, Command::CloseTab),
    ("1", None, Command::SetView(View::Grid)),
    ("2", None, Command::SetView(View::List)),
    ("3", None, Command::SetView(View::Columns)),
    ("4", None, Command::SetView(View::Gallery)),
    ("=", None, Command::Zoom(1)),
    ("+", None, Command::Zoom(1)),
    ("-", None, Command::Zoom(-1)),
    ("0", None, Command::Zoom(0)),
];

/// Bindings for named keys and modifier chords, which never wait for a sequence.
pub fn chord(key: &Key, modifiers: Modifiers) -> Option<Command> {
    let shift = modifiers.shift();
    if modifiers.ctrl() {
        return control(key, shift);
    }
    if modifiers.alt() {
        return ALT
            .iter()
            .find(|(bound, _)| bound == key)
            .map(|(_, command)| command.clone());
    }
    NAMED
        .iter()
        .find(|(bound, needs_shift, _)| bound == key && *needs_shift == shift)
        .or_else(|| {
            NAMED
                .iter()
                .find(|(bound, needs_shift, _)| bound == key && !needs_shift)
        })
        .map(|(_, _, command)| command.clone())
}

fn control(key: &Key, shift: bool) -> Option<Command> {
    if *key == Key::Tab {
        return Some(if shift {
            Command::PreviousTab
        } else {
            Command::NextTab
        });
    }
    let Key::Character(text) = key else {
        return None;
    };
    let text = text.to_lowercase();
    CONTROL
        .iter()
        .find(|(bound, needs_shift, _)| {
            *bound == text && needs_shift.is_none_or(|needs| needs == shift)
        })
        .map(|(_, _, command)| command.clone())
}

enum Argument {
    None(Command),
    Required(fn(String) -> Command),
}

const CONSOLE: &[(&str, Argument)] = &[
    ("dl", Argument::Required(Command::Download)),
    ("mkdir", Argument::Required(Command::MakeFolder)),
    ("touch", Argument::Required(Command::MakeFile)),
    ("rename", Argument::Required(Command::RenameTo)),
    ("open_with", Argument::Required(Command::OpenWith)),
    ("shell", Argument::Required(Command::OpenWith)),
    ("delete", Argument::None(Command::Delete)),
    ("trash", Argument::None(Command::Trash)),
    ("restore", Argument::None(Command::Restore)),
    ("empty_trash", Argument::None(Command::EmptyTrash)),
    ("tab_new", Argument::None(Command::NewTab)),
    ("tab_close", Argument::None(Command::CloseTab)),
    ("q", Argument::None(Command::CloseTab)),
    ("terminal", Argument::None(Command::Terminal)),
    ("hidden", Argument::None(Command::ToggleHidden)),
    ("gallery", Argument::None(Command::SetView(View::Gallery))),
    ("pin", Argument::None(Command::PinCurrent)),
    ("clear_history", Argument::None(Command::ClearHistory)),
    ("scripts", Argument::None(Command::ListScripts)),
    ("script", Argument::Required(Command::RunScript)),
    ("title", Argument::Required(|text| Command::EditMetadata("title", text))),
    ("caption", Argument::Required(|text| Command::EditMetadata("caption", text))),
    ("source", Argument::Required(|text| Command::EditMetadata("source", text))),
    ("tags", Argument::Required(|text| Command::EditMetadata("tags", text))),
    ("note", Argument::Required(|text| Command::EditMetadata("notes", text))),
    ("creator", Argument::Required(|text| Command::EditMetadata("creator", text))),
    ("help", Argument::None(Command::Help)),
];

/// Parses a console line such as `mkdir drafts` or `open_with gimp`.
pub fn parse_console(line: &str) -> Result<Command, String> {
    let line = line.trim_start();
    let (name, argument) = line.split_once(' ').unwrap_or((line, ""));
    let argument = argument.trim();
    match name {
        "" => return Err(String::from("Type a command")),
        "cd" => {
            return Ok(Command::Cd(
                if argument.is_empty() { "~" } else { argument }.to_owned(),
            ));
        }
        "sort" => return sort_command(argument),
        _ => {}
    }
    let (_, kind) = CONSOLE
        .iter()
        .find(|(bound, _)| *bound == name)
        .ok_or_else(|| format!("Unknown command: {name}"))?;
    match kind {
        Argument::None(command) => Ok(command.clone()),
        Argument::Required(_) if argument.is_empty() => Err(format!("{name} needs an argument")),
        Argument::Required(command) => Ok(command(argument.to_owned())),
    }
}

fn sort_command(argument: &str) -> Result<Command, String> {
    Ok(Command::SortBy(match argument {
        "name" | "natural" => Sort::Name,
        "size" => Sort::Size,
        "mtime" | "modified" => Sort::Modified,
        "type" | "extension" => Sort::Type,
        "reverse" => return Ok(Command::Reverse),
        _ => return Err(String::from("sort: name, size, mtime, type, or reverse")),
    }))
}

/// Help rows: keys, then what they do.
pub const HELP: &[(&str, &str)] = &[
    ("h j k l", "Parent, down, up, open"),
    ("arrows", "Move; left and right step in the grid"),
    ("gg  G", "First, last"),
    ("Ctrl+D  Ctrl+U", "Half page down, up"),
    ("H  L", "History back, forward"),
    ("Space  v  uv", "Mark, invert, clear marks"),
    ("yy  dd  pp", "Copy, cut, paste"),
    ("dT  dD", "Trash, delete permanently"),
    ("cw  F2", "Rename"),
    ("/", "Filter this folder"),
    (":", "Command: mkdir, touch, cd, dl, open_with, sort, pin, script, caption"),
    (
        "on os om ot or",
        "Sort by name, size, modified, type; reverse",
    ),
    ("zh  .", "Hidden files"),
    ("m<key>  '<key>", "Set bookmark, jump to bookmark"),
    ("gn  gt  gT  q", "New, next, previous, close tab"),
    ("gh  g/  gx", "Home, root, trash"),
    ("S  F4", "Terminal here"),
    ("i  zs", "Preview pane, sidebar"),
    ("Ctrl+1 2 3 4", "Grid, list, columns, gallery"),
    ("zg  :gallery", "Toggle gallery or enter gallery"),
    ("Gallery: Enter Esc", "Focus image, return to overview"),
    ("Gallery: h/l", "Previous/next in focus or grid"),
    ("Ctrl+L", "Edit path"),
    ("+  -  Ctrl+wheel", "Zoom the grid; Ctrl+0 resets"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_wait_for_prefixes_and_resolve_complete_bindings() {
        assert_eq!(resolve("g"), Resolution::Pending);
        assert_eq!(resolve("gg"), Resolution::Run(Command::Top));
        assert_eq!(resolve("zg"), Resolution::Run(Command::ToggleGallery));
        assert_eq!(resolve("d"), Resolution::Pending);
        assert_eq!(resolve("dT"), Resolution::Run(Command::Trash));
        assert_eq!(resolve("dD"), Resolution::Run(Command::Delete));
        assert_eq!(resolve("o"), Resolution::Pending);
        assert_eq!(resolve("os"), Resolution::Run(Command::SortBy(Sort::Size)));
        assert_eq!(resolve("x"), Resolution::Unknown);
        assert_eq!(resolve("gq"), Resolution::Unknown);
    }

    #[test]
    fn bookmarks_accept_any_key() {
        assert_eq!(resolve("m"), Resolution::Pending);
        assert_eq!(resolve("ma"), Resolution::Run(Command::SetBookmark('a')));
        assert_eq!(resolve("'a"), Resolution::Run(Command::Bookmark('a')));
        assert_eq!(resolve("'1"), Resolution::Run(Command::Bookmark('1')));
    }

    #[test]
    fn chords_cover_thunar_shortcuts() {
        let ctrl = Modifiers::CONTROL;
        let shift = Modifiers::SHIFT;
        assert_eq!(
            chord(&Key::Character("c".into()), ctrl),
            Some(Command::Yank)
        );
        assert_eq!(
            chord(&Key::Character("V".into()), ctrl),
            Some(Command::Paste)
        );
        assert_eq!(
            chord(&Key::Character("N".into()), ctrl | shift),
            Some(Command::Console("mkdir "))
        );
        assert_eq!(
            chord(&Key::Delete, Modifiers::empty()),
            Some(Command::Trash)
        );
        assert_eq!(chord(&Key::Delete, shift), Some(Command::Delete));
        assert_eq!(chord(&Key::ArrowLeft, Modifiers::ALT), Some(Command::Back));
        assert_eq!(
            chord(&Key::ArrowRight, Modifiers::empty()),
            Some(Command::Step(1))
        );
        assert_eq!(
            chord(&Key::ArrowDown, Modifiers::empty()),
            Some(Command::Move(1))
        );
        assert_eq!(chord(&Key::Character("j".into()), Modifiers::empty()), None);
        assert_eq!(chord(&Key::Character("4".into()), ctrl), Some(Command::SetView(View::Gallery)));
    }

    #[test]
    fn console_lines_parse_with_arguments() {
        assert_eq!(
            parse_console("mkdir new folder"),
            Ok(Command::MakeFolder("new folder".into()))
        );
        assert_eq!(parse_console("cd"), Ok(Command::Cd("~".into())));
        assert_eq!(parse_console("sort size"), Ok(Command::SortBy(Sort::Size)));
        assert_eq!(parse_console("gallery"), Ok(Command::SetView(View::Gallery)));
        assert_eq!(
            parse_console("open_with gimp -n"),
            Ok(Command::OpenWith("gimp -n".into()))
        );
        assert!(parse_console("rename ").is_err());
        assert!(parse_console("explode").is_err());
    }
}
