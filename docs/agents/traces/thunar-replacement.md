# Thunar replacement

Goal: turn Explorer from a browse-only explorer into a lightweight Thunar replacement. It stays in Rust with Dioxus Native, keeps Tumbler thumbnails, uses ranger as the feature benchmark, and follows the minimal reference look: text-only navigation, no borders, whitespace, soft thumbnails, short animations.

Predecessor trace: `native-explorer.md` (browse-only version, verified 2026-10-12).

## Done criteria

- File operations: copy, cut, paste with progress, rename, new folder or file, trash, permanent delete with confirmation, restore and empty trash. Nothing is overwritten silently.
- Selection: cursor plus marks (Space, v, uv, Ctrl+A, Ctrl/Shift click). Operations act on marks, or on the cursor item when nothing is marked.
- Ranger parity subset: hjkl, gg/G, Ctrl+D/U, yy/dd/pp, dT/dD, cw, o-sort keys, zh, m/' bookmarks, gn/gt/gT tabs, `:` console, Miller-column view with preview, per-directory cursor memory, S terminal, `?` help.
- Thunar parity subset: GTK bookmarks and mounts in the sidebar, `.hidden` files, Trash place, context menu, tabs, F2/Delete/Shift+Delete/Ctrl+C/X/V, mouse back/forward buttons, live refresh through a filesystem watcher.
- Large folders render through a virtual viewport. The 120-item paging is removed, and keyboard movement keeps the cursor visible.
- Tumbler: grid thumbnails (`large`) and preview pane (`x-large`).
- Verified by unit tests, native Blitz event tests, clippy, and screenshots.

## Decisions

- Blitz 0.2 delivers only mouse, click, key, and input DOM events. It has no wheel, resize, dblclick, contextmenu, or scroll-into-view. Components cannot reach the document.
  - `src/shell.rs` runs `blitz_shell::BlitzApplication` directly instead of `dioxus_native::launch_cfg`, whose application keeps `inner` private. It publishes the logical window size and wheel deltas to the UI.
  - The UI owns scrolling: a virtual viewport renders only visible rows and follows the cursor.
  - Double-click is detected from `click` timing. The context menu opens on secondary-button `mousedown`.
- Trash uses `gio trash` (GLib, the same stack Thunar uses). No `trash` crate is cached offline.
- Copy and move use std, so progress can be reported. A move falls back to copy then remove on EXDEV. Name conflicts get "name (copy N)" and never overwrite.

## Status (2026-10-13)

Implemented and verified. All done criteria are met, except where Limits in README.md says otherwise.

## Files

- New: `src/shell.rs`, `src/operations.rs`, `src/places.rs`, `src/preview.rs`, `src/watch.rs`, `src/ui/{state,commands,keymap,keyboard,viewport,views,preview,sidebar,topbar,overlay}.rs`, `assets/explorer.desktop`.
- Rewritten: `src/filesystem.rs` (natural sort, sort order, `.hidden`, permissions, path resolution), `src/icons.rs`, `src/ui/mod.rs`, `assets/style.css`, `src/screenshot/native_tests.rs`, README.
- Removed: `src/ui/{content,toolbar,action}.rs` and the Gambarino font, which only the old heading used.

## Verification

- `cargo test --offline`: 36 passed. `-- --ignored`: Tumbler and the real gio trash/restore round trip passed.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`: clean.
- `python scripts/native_smoke.py`: passed. The palette check now recolours `primary_fixed_dim`, which the folder art uses.
- Native event test: grid arrows (row steps), l/h with cursor memory, zh, console mkdir, filter, yy/pp producing "(copy)", dD with confirm, cw rename, list and columns views with text preview, right-click menus, zoom, tabs, help, a bookmark, and a 500-item folder through the virtual viewport. Captures were reviewed.
- Live Hyprland window: resize events reach the layout (1880 px, then 1040 px tiled). Wheel scroll and Ctrl+wheel zoom were sent through ydotool and checked with grim. The Wayland class is `explorer`.
- Release: 33 MB binary, 0% idle CPU, 228 MB RSS on an empty folder (NVIDIA libraries 111 MB, other libraries 56 MB, heap 38 MB). Thunar uses 66 MB.

## Root causes found

- Blitz clears focus on any click outside an input, and key events then target `<html>`. The app root refocuses a hidden button after clicks unless a text field is active.
- Parley keeps the caret at 0 in a field mounted with text. `shell::apply_caret_request` sends an End key to the newly focused field. The native tests apply the same hook.
- `grid-template-columns: repeat(n, 1fr)` with percentage-width items gives the first track the full width in Blitz. Tracks are now exact pixel widths from the layout.
- Blitz ignores `white-space: nowrap` with ellipsis on these labels. `filesystem::shorten` truncates in the middle and keeps the extension.
- Blitz paints `box-shadow: inset` as a fill. Sheets use a border.
- A transform transition did not advance in headless capture. Folder art is sized with `calc()`.
- ydotool on this machine needs `YDOTOOL_SOCKET=/run/user/1000/.ydotool_socket`, and absolute moves need `hyprctl dispatch 'hl.dsp.cursor.move({x=..,y=..})'`.

## Next steps

Persist settings (zoom, view, bookmarks). Exchange clipboard data with other apps through `x-special/gnome-copied-files` (needs wl-clipboard or a newer Blitz). Build an open-with menu from `gio mime`. Keep the scroll anchor while zooming. Use the CPU renderer once Blitz supports one.

## Round 2 (2026-10-13): rename, clipboard, drag and drop, CPU rendering, install

User asks: universal Wayland copy and paste, drag and drop, uncropped images, rename to "explorer", CPU rendering with a small footprint, then install in place of Thunar. The user chose to keep Dioxus and add a Wayland layer over switching to GTK4 or a webview. The user later asked to stop optimizing at a good baseline and record ideas in the README.

- Renamed crate, binary, title, Wayland app ID, and desktop entry to `explorer`.
- `src/raster.rs`: CPU raster through `vello_cpu` with adaptive workers (0 at rest, 4 for frame bursts, freed with `malloc_trim`). `--renderer=cpu|gpu`, default cpu. `anyrender_vello_cpu` 0.8.1 is the version built on anyrender 0.6 (0.6.0 targets anyrender 0.5).
- `src/bench.rs`: `--bench` scrolls off-screen through key events. Earlier I measured with ydotool on the live desktop, which moved the user's cursor; the user objected, and the in-app benchmark replaced it.
- Thumbnails: aspect ratio kept (`max-width`/`max-height`), and the flavor is picked from the drawn size.
- `src/clipboard.rs`: GTK file clipboard formats through wl-clipboard-rs data-control. It is enabled only in the windowed app, so tests never touch the session clipboard.
- `src/dnd.rs`: second queue on winit's wl_display (wayland-backend client_system). Drag source from our button serial; drop target accepts text/uri-list. `src/ui/drag.rs` holds the hit test, the drop highlight, and the 8 px drag threshold. `operations::drop_mode` applies Thunar's same-filesystem rule.
- `src/file_manager1.rs`: `--dbus-service` serves org.freedesktop.FileManager1 and exits after 60 s idle. `--select=PATH` opens a folder with PATH selected.
- `scripts/headless_wayland.sh`: runs a command inside a private headless sway, used for clipboard and window checks without touching the desktop.

Verification: 42 unit and native tests pass, and clippy is clean. Clipboard round trip and cross-app formats (wl-paste and wl-copy) pass in headless sway. The release window runs under headless sway with drag and drop attached. FileManager1 ShowItems spawned `explorer --select=…` on a private bus. A real pointer drag was not automated: the headless session has no input devices, and the user's desktop was off limits.

Installed: `~/.local/bin/explorer`, `~/.local/share/applications/explorer.desktop`, xdg-mime defaults for inode/directory and x-directory/normal, and `~/.local/share/dbus-1/services/org.freedesktop.FileManager1.service`. dbus-broker now ignores Thunar's duplicate entry. In `~/.config/hypr/hyprland.lua`, `fileManager` is the explorer path. Backups: `hyprland.lua.bak-before-explorer` and `mimeapps.list.bak-before-explorer`.
