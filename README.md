# Explorer

Explorer is a file manager for Linux, written in Rust with Dioxus Native (Blitz HTML/CSS and Vello). It is meant to replace Thunar. It uses ranger as the benchmark for keyboard work, and it uses Tumbler, the service Thunar uses, for thumbnails.

## Run

```sh
cargo run --locked -- "/path/to/folder"
cargo build --locked --release && ./target/release/explorer
```

Without a folder argument, Explorer opens your home folder. A `file://` URI also works.

Options: `--renderer=cpu|gpu` (or `EXPLORER_RENDERER`), `--select=PATH` to open a folder with PATH selected, `--screenshot[=FILE]`, `--bench`, and `--dbus-service`.

### Install as the default file manager

```sh
cargo build --locked --release
install -Dm755 target/release/explorer ~/.local/bin/explorer
install -Dm644 assets/explorer.desktop ~/.local/share/applications/explorer.desktop
xdg-mime default explorer.desktop inode/directory x-directory/normal
mkdir -p ~/.local/share/dbus-1/services
printf '[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec=%s/.local/bin/explorer --dbus-service\n' "$HOME" \
    > ~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service
```

The D-Bus service answers "Show in folder" from browsers and other apps, which otherwise starts Thunar. In Hyprland, point the file manager bind at `~/.local/bin/explorer`. To switch back, run `xdg-mime default thunar.desktop inode/directory x-directory/normal` and delete the service file.

## Features

- Views: grid with Tumbler thumbnails, list with size and date, ranger-style Miller columns (parent, current, preview), and image-only gallery. Ctrl+1, Ctrl+2, Ctrl+3, Ctrl+4 switch views.
- Grid zoom: Ctrl+wheel, Ctrl+= and Ctrl+-, or `+` and `-`. Ctrl+0 resets. Maximum zoom is 4.5×, twice the previous maximum.
- File operations: copy, cut, and paste with progress and cancel; rename; new folder or file; move to trash; permanent delete after confirmation; restore and empty trash. Pasting never overwrites; a conflicting item becomes "name (copy).ext".
- Selection: a cursor plus marks. Space, `v`, `uv`, Ctrl+A, Ctrl+click, Shift+click, and Shift+arrows. Operations act on the marks, or on the cursor item when nothing is marked.
- Sidebar: home and XDG folders, GTK bookmarks (the list Thunar shows), mounted volumes, file system root, and Trash. Folders listed in a `.hidden` file stay hidden, as in Thunar.
- Tabs (gn, Ctrl+T, middle click), bookmarks (`m` and `'`), per-folder cursor memory, history, live refresh when files change, a `:` command line, an open-with command, a terminal here, and a new window (Ctrl+N).
- System clipboard: copy and cut publish `x-special/gnome-copied-files`, `text/uri-list`, and plain paths through the Wayland data-control protocol, so files move between Explorer, Thunar, Nautilus, and other apps in both directions. A paste reads whichever of those formats is on the clipboard. Encoded clipboard images become `clipboard.png`, `.jpg`, `.webp`, or the matching image extension. Plain text becomes `clipboard.txt`. A single HTTP or HTTPS URL starts a download.
- Text fields, including the `:` command line, paste Wayland text with Ctrl+V at the caret or over the selection. Ctrl+A selects the whole field.
- Drag and drop with other apps on Wayland: drag files out, or drop files in. A drop on a folder goes into that folder; elsewhere it goes into the open folder. As in Thunar, a drop moves within one filesystem and copies across filesystems.
- Thumbnails keep the image's aspect ratio. Explorer requests the smallest Tumbler size that covers the drawn size: 128, 256, or 512 px.
- Preview pane (`i`): folder contents, a 512 px Tumbler thumbnail, or the first lines of a text file.
- Gallery (`Ctrl+4`, `:gallery`, or `zg` to toggle): image-only overview preserving the current folder sort; Enter or double-click focuses the original image, with a filmstrip. Escape returns to the overview. In the gallery, h/l and left/right step between images; i toggles focus metadata. Zoom uses +, -, or Ctrl+wheel. Return to the prior view with `zg`.
- Matugen colours from `~/.config/matugen/colors.css`, plus light and dark themes. Hover the sidebar foot to switch the theme or turn motion off.

## File-first collections and Rust extensions

Pinned folders use the same GTK bookmarks file as Thunar. The collapsible
Quick Access section shows recent files and frequently visited folders.
History is kept locally in `$XDG_STATE_HOME/explorer/history.json` (at most
256 entries). Set `EXPLORER_DISABLE_HISTORY=1` to stop recording and run
`:clear_history` to erase prior history. Keyboard bookmarks persist between runs.

Gallery annotations can be changed in the focused image viewer or through
`:title`, `:caption`, `:source`, `:tags`, `:note`, and `:creator`.
Explorer stores these annotations in the hidden folder-local `.explorer/metadata`
directory. Existing EXIF/XMP image bytes are not altered; native XMP
read/write is not yet implemented. A `.explorer/board.json` data schema
is available for future manually arranged boards, but no board canvas yet.

Custom importers and organizers can be written as separate Rust binaries,
installed via JSON manifests in `~/.config/explorer/extensions`,
listed with `:scripts` and run with `:script NAME ARGUMENTS`. Extensions
are trusted programs, not sandboxed plugins. See [the extension protocol](docs/extensions.md)
and the [sample annotations importer](scripts/extensions/import_annotations).

## Keys

Press `?` in the app for the full list.

| Keys | Action |
| --- | --- |
| `h` `j` `k` `l` | Parent, down, up, open |
| Arrows | Up and down move. In the grid they move by row, and left and right step between items. In list and columns views, left and right go to the parent or open. |
| `gg` `G`, Ctrl+D Ctrl+U | First, last, half page down and up |
| `H` `L`, Alt+Left Alt+Right | History back and forward |
| Space, `v`, `uv` | Mark, invert marks, clear marks |
| `yy` `dd` `pp`, Ctrl+C Ctrl+X Ctrl+V | Copy, cut, paste |
| `dT` or Delete | Move to trash |
| `dD` or Shift+Delete | Delete permanently, after a y/n prompt |
| `cw`, F2 | Rename |
| `/` | Filter this folder |
| `:` | Command: `mkdir`, `touch`, `rename`, `cd`, `dl`, `open_with`, `sort`, `restore`, `empty_trash` |
| `r` | Open with a typed command; the selected files are passed as arguments |
| `on` `os` `om` `ot` `or` | Sort by name, size, modified, type; reverse |
| `zh`, `.`, Ctrl+H | Hidden files |
| `ma`, `'a`, `''` | Set bookmark a, jump to it, jump back |
| `gn` `gt` `gT` `q` | New, next, previous, close tab |
| `gh` `g/` `gx` | Home, root, trash |
| `S`, F4 | Terminal in this folder (`$TERMINAL` or `xdg-terminal-exec`) |
| `i`, `zs` or F9 | Preview pane, sidebar |
| `Ctrl+4`, `:gallery`, `zg` | Gallery view; `zg` returns to the previous view |
| Gallery: `Enter`, `Esc`, `h`/`l` | Focus image, return to overview, previous/next image |
| Ctrl+L | Edit the path; clicking the current folder name does the same |

The mouse follows Thunar. A click selects. A double click opens. A right click opens a menu for the item or for the folder. A middle click opens a folder in a new tab. The back and forward buttons move through history.

## Capture and verify

```sh
./target/debug/explorer "/path/to/folder" --screenshot=/tmp/explorer.png
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -- --ignored        # real Tumbler, trash, and clipboard round trips
scripts/headless_wayland.sh cargo test --locked -- --ignored   # same, with a private sway clipboard
python scripts/native_smoke.py
EXPLORER_CAPTURE_DIR=/tmp/caps cargo test --locked native   # PNG at each test checkpoint
scripts/headless_wayland.sh cargo test --locked screenshot::regressions::native_editor_pastes_wayland_text_at_caret -- --ignored --exact
scripts/headless_wayland.sh python scripts/window_regressions.py target/release/explorer
```

`--screenshot` renders through the app's own Blitz and Vello path and needs no compositor. The native test sends real Blitz key and mouse events through file operations, menus, views, zoom, tabs, and a 500-item folder.

## Design notes

Blitz 0.2 sends no wheel, resize, or double-click DOM events, and it cannot scroll an element into view. Explorer therefore runs its own event loop (`src/shell.rs`) around `blitz_shell`. That loop reports the window size and wheel input to the UI, and the UI draws only the visible rows. Folders of any size open without paging, and the cursor stays visible during keyboard movement.

winit 0.30 has no Wayland drag and drop. `src/dnd.rs` opens a second event queue on winit's own `wl_display` and uses the core `wl_data_device` protocol on the window's surface. The clipboard (`src/clipboard.rs`) uses data-control through `wl-clipboard-rs`, so it works without window focus.

The locked Blitz and AccessKit versions carry local fixes for retained layout wrappers, node-removal interaction handles, missing inline layout during hit testing, clipboard initialization, resource delivery after shutdown, and unavailable accessibility services. Sources, licenses, and patch provenance are in [vendor/README.md](vendor/README.md). Reproduction evidence and remaining limits are in [the crash trace](docs/agents/traces/clipboard-crashes.md).

## Downloads

Type `:dl URL` to download into the current folder. Explorer uses the response's filename or a filename in the URL. When neither supplies a filename, a **Save as** field opens with an extension from the response's content type. Enter saves; Escape cancels. You can also type `:dl URL my image.webp` to supply a name, including spaces.

Downloads use the installed [`curl`](https://curl.se/docs/manpage.html). They run in the background and can be cancelled from the progress bar. HTTP errors remove the staged data. Existing files and symlinks are preserved; conflicts get numbered copy names. Clipboard bytes use the same save path. Temporary staging stays in the destination folder and is removed after saving or cancelling.

## Rendering and performance

Explorer uses the GPU by default. `--renderer=cpu` or `EXPLORER_RENDERER=cpu` selects software rendering. `--renderer` takes precedence over the environment. CPU mode uses one thread for isolated repaints and 4 workers for frame bursts. The worker pool survives per-frame resets and is released at the next isolated repaint. `EXPLORER_RENDER_THREADS` sets its size; 0 disables workers.

Wheel and touchpad input now moves the viewport immediately. Keyboard navigation retains eased cursor following. Nothing repaints continuously while idle.

`explorer DIR --bench [--renderer=cpu|gpu]` runs off-screen. It sends wheel events through the window's input channel, resets the renderer after each frame as the window does, and reports first movement, settling, layout, painting, p95 frame time, CPU, and memory as JSON. It samples at a 144 Hz cadence and forces 240 paints to measure drawing cost. Frame time is processing cost, not measured compositor FPS. Wheel timing measures document input to rendered pixels, excluding hardware input and compositor presentation. `-1` means the folder could not scroll by a full notch.

Before/after results and commands are in [the task trace](docs/agents/traces/scroll-and-import.md). Run the benchmark serially, without builds or other test jobs competing for CPU. The old key benchmark omitted the window renderer's reset and understated its CPU cost.

Use `scripts/headless_wayland.sh COMMAND` for window and clipboard tests. Its private runtime directory prevents it from selecting a desktop socket. Do not use `scripts/measure_renderers.py` while someone is using the desktop; that older script moves the real pointer.

## Limits

- A drag shows the compositor's default cursor; there is no drag preview image. Drops on sidebar places are not supported yet.
- Clipboard and drag and drop need a Wayland session. On X11, copy and paste stay inside the app and drag and drop is unavailable.
- There is no general undo or redo yet; extension file operations are also not transactional.
- Unmounted drives do not appear until something mounts them.
- "Open with" takes a typed command, not a list of installed applications.
- Sorting, grid zoom, current view, sidebar/preview state, and keyboard bookmarks persist between runs. Tabs and marked selections reset when the app restarts.
- Zooming moves the view back to the cursor.

For optional embedded XMP import/export, install Exiv2 and use
`:xmp_read` / `:xmp_write` on a selected image. Sidecars remain the source
of truth; Exiv2 is not required for ordinary browsing.
