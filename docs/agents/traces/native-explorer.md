# Native file explorer

Goal: a local Linux file explorer using Dioxus Native and Blitz HTML/CSS, with the supplied visual references, Matugen colors, bundled Fontshare fonts, Thunar's thumbnail service, ranger-style navigation, and app-owned screenshots.

Status: implemented and verified for the browsing and capture scope. The initial workspace was empty. This version browses and opens files; it does not copy, move, rename, delete, or trash them.

## Implementation

- Cargo configuration and lockfile pin Dioxus Native 0.7.10. The published release requires hot-reload's optional CLI config dependency even without a dev server. Unused file-dialog and HTML-parser features stay off.
- `src/filesystem.rs` preserves native paths, including non-UTF-8 names and links, reads directories off the UI thread, filters and sorts folders first, maintains navigation history, and opens files through `xdg-open`. Rendering is paged at 120 entries.
- `src/ui/` contains the browsing components, accessible keyboard-activated buttons, navigation, search, view controls, and details pane. `assets/style.css` supplies the layout, palette tokens, folder artwork, and restrained motion.
- `src/theme.rs` reads and watches the existing Matugen GTK palette at the XDG config path. Folder art, badges, icons, text, and surfaces use its tokens. Original Gambarino, General Sans, and Satoshi TTF files and Fontshare licenses are bundled. Installed monospace fonts handle paths and metadata.
- `src/thumbnails.rs` queues supported files through Tumbler's session D-Bus API and reuses the standard URI-hashed thumbnail cache. Cache validation checks source readability, URI, and integer or fractional source timestamps. No custom thumbnail decoder remains.
- `src/cli.rs` parses the initial directory, `--screenshot`, `--screenshot=FILE.png`, help, and the positional terminator. Existing screenshot files are preserved.
- `src/screenshot.rs` creates the same Dioxus app, settles its live Blitz document, and renders it through the existing Vello GPU image renderer. It writes a 1120 × 740 PNG and exits. It has no window-server or compositor capture dependency. It waits for attached DOM readiness, resource loading, and a quiet interval, with a 20-second timeout.
- `scripts/native_smoke.py` uses the actual screenshot flags with an isolated directory and palette. The original PNG remains intact; an in-memory enlarged copy helps OCR read the small UI text. No Hyprland, grim, or wtype calls remain.
- README documents setup, capture, fonts, Matugen, Tumbler, bindings, checks, and scope.

## Verification

- `cargo fmt --check`: passed.
- `cargo test --locked --offline`: 11 passed; the service-dependent test is intentionally separate.
- `cargo test --locked --offline tumbler_creates_a_valid_shared_thumbnail -- --ignored`: passed against the installed service, including creation and validation of its actual shared cache file.
- `cargo clippy --locked --offline --all-targets -- -D warnings`: passed.
- `cargo build --locked --offline`: passed.
- Native document event test: gg/l/h navigation, search autofocus and input, return to navigation, selection details, hidden files, list view, Ctrl+L path editing, and first/last-page jumps passed. The test also checks that all five real TTF font resources load through Blitz.
- `python scripts/native_smoke.py`: passed with both WAYLAND_DISPLAY and DISPLAY blank. Checked default and explicit capture paths, populated grid, visible thumbnail filename, changed Matugen accent pixels, missing-directory error UI, and refusal to overwrite an existing PNG.
- Final captures: `/tmp/folio-native-qa-_xltlmgr/folio-screenshot.png`, `matugen.png`, `palette-update.png`, and `error.png`. Inspected the actual PNG with the app's bundled fonts and Tumbler preview.
- Normal native window launch stayed open for three seconds without a panic; the test process was then closed.
- Existing quality engine reports Rust CC maximum 9 and cognitive maximum 10. MI and CRAP were not collected by its Rust adapter; no claims are made about those metrics. All handwritten files are below 1000 lines.

## Root causes resolved

- Tumbler emits fractional microsecond Thumb::MTime metadata on this machine; validation now accepts that precision and standard whole seconds.
- Cargo feature unification with zbus's Tokio backend broke AccessKit threads without a reactor. The default async-io backend works with the app runtime.
- Blitz 0.2 does not fetch font-face sources from inline style elements. Fonts load through a linked data-URL stylesheet; TTF avoids an additional WOFF decoder dependency.
- Blitz computes focusability before Dioxus assigns tabindex. A native button supplies the navigation focus target. Keyed single-item fragments remount input targets so native autofocus runs when search, address, or command focus is requested.
- The thumbnail filename clipped under intrinsic flex sizing. Giving grid filenames an explicit card width restores the label.
- Detached DOM nodes must be excluded from capture-readiness and native text assertions.

## Remaining limits

Release startup, memory, and large-directory performance have not been benchmarked. The Matugen watcher is implemented; the final screenshot suite verifies palette consumption across captures rather than a live watcher update in an already-open window. Other Tumbler media types depend on installed plugins. Screenshot mode captures the initial explorer state at a fixed size and requires a working graphics backend. It does not attach to another running Folio process.

Next executable step: `cargo run --locked -- --screenshot`, or run with an initial directory. Choose a new output filename when the default already exists. All work is local; there is no git repository or remote publication in this workspace.
