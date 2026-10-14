# Scrolling, zoom, and folder imports

Goal: measure scrolling through the real wheel and renderer lifecycle, remove measured bottlenecks, double maximum zoom, and support clipboard images/text plus HTTP downloads into the current folder.

Done criteria: before/after headless measurements, regression tests through native events, safe names and no overwrites, background downloads with cancellation, clear Save as prompt for unnamed downloads, clean affected checks.

Constraints: use `--bench`, app-owned captures, and `scripts/headless_wayland.sh`. Never send input to the desktop. This folder has no Git worktree. Original sources are backed up in `/tmp/explorer-before`.

Findings:

- Old scrolling eases 32% of the remaining distance every 16 ms. It takes 13 steps to settle a 56 px notch, before renderer delays.
- `SoftbufferWindowRenderer::render` calls `ImageRenderer::reset` after every presented frame. Our reset dropped the worker pool. The old benchmark omitted reset, so its CPU results did not exercise the window lifecycle.
- Original offscreen key benchmark on Pictures: CPU mean 19.36 ms, p95 24.67 ms, layout 2.11 ms; GPU mean 5.76 ms, p95 7.52 ms, layout 2.21 ms. These are comparison context only, not a wheel baseline.
- The obsolete desktop measurement script was mistakenly started before reading the previous trace. It has exited. Its results are invalid because it selected an existing user's Explorer window. No further desktop input is permitted.

Status: implementation and verification complete. With explicit user permission, the verified release executable was atomically installed into `~/.local/bin/explorer`. Hyprland's fileManager points to that path and the desktop launcher resolves to it. Installed and release SHA256 match: `6d5d6c9b3f0c7edcd00a7f3fa8f6afb6c066fe33ce7809afdce068c274e74dc0`. Installed `--help` confirms GPU default. The previous executable is backed up at `/tmp/explorer-installed-backup-4ovwzjtw/explorer`. Existing windows were not restarted. Installed curl handles HTTP/TLS without a new dependency.

## Implemented

- Wheel input applies its target in the first UI poll. Keyboard cursor following keeps its easing.
- CPU per-frame reset retains workers, with a regression for reset and later idle release.
- GPU is the default, approved by the user after comparing rendering cost. CPU flags and environment override remain available.
- Maximum grid zoom rises from 2.25 to 4.5. Grid geometry and artwork stay inside narrow panes.
- Clipboard files keep their transfer semantics. Images and text save without overwrites. A single HTTP URL starts a download.
- `:dl URL [filename]` downloads using installed curl, restricting initial and redirected protocols to HTTP(S). Headers supply names; ambiguous URLs open Save as. Staging uses a private directory in the destination, with atomic no-overwrite hard-link publication and cleanup on error, cancel, or prompt dismissal.
- Paste pins the destination before asynchronous clipboard reads. Saving after navigation does not move the new folder's cursor.
- Headless Wayland now uses a private runtime directory and discovers only sockets within it. WAYLAND_SOCKET is unset for clients; an empty value prevented wayland-client from connecting.

## Serial measurements

The same release profile and 1120 × 740 viewport were used. Two runs per CPU configuration, with no simultaneous build or test. JSON is in `/tmp/explorer-perf/*-cpu-serial.json`.

| Folder | Original CPU mean | Fixed CPU mean | Original wheel settle | Fixed wheel settle |
| --- | --- | --- | --- | --- |
| Pictures, 53 entries | 24.52 / 24.46 ms | 19.66 / 19.96 ms | 378.66 / 380.48 ms | 33.77 / 28.78 ms |
| 500 text files | 14.31 / 13.60 ms | 10.15 / 11.44 ms | 292.72 / 264.54 ms | 15.01 / 16.63 ms |

Idle redraw count was zero. CPU workers raise memory during continuous drawing (Pictures about 114–115 MB RSS), then an isolated repaint releases it (55–57 MB).

Rejected experiment: a bounded premultiplied thumbnail cache required a PaintScene adapter and produced no repeatable improvement. Pictures means were 21.28 / 19.99 ms; 500-file means were 11.00 / 10.62 ms. The adapter and its test were removed.

Download domain tests cover names, byte preservation, conflicts, invalid names, redirect headers, protocol restrictions, HTTP failures, and cancellation. Clipboard file/image/text round trips pass inside private Wayland. A private GPU window appears and remains alive; the isolated accessibility registry is unavailable and AccessKit logs a background thread failure. Accessibility integration with the desktop was not verified.

Native UI failure and fix: submitting a console command unmounts Console. Dioxus `spawn` tasks belong to the calling component and are cancelled on unmount; the download never started and the job remained. Clipboard URL paste from the app scope worked. File operations now use the existing `spawn_forever`, which schedules into the application root. Native console tests check saved bytes, explicit names, and prompt dismissal.

## Final release measurements

Serial runs with the final release build, Pictures (53 entries), and the same 1120 × 740 viewport. Results: `/tmp/explorer-perf/final-pictures-cpu.json` and `final-pictures-gpu.json`.

| Renderer | Mean frame processing | p95 | First movement / settle | Active / after-idle RSS |
| --- | --- | --- | --- | --- |
| Fixed CPU | 19.92 / 19.68 ms | 22.83 / 22.50 ms | 31.07 / 28.56 ms | 113 / 55 MB |
| Default GPU | 5.08 / 5.04 ms | 6.06 / 6.28 ms | 8.59 / 6.74 ms | 254 / 254 MB |

GPU processing is about 79% shorter than the original CPU mean of 24.49 ms. Both GPU p95 values fit the 6.94 ms budget at 144 Hz. This measures application processing throughput, not actual desktop FPS or physical-input-to-display latency. Idle redraw count was zero in both configurations. The GPU uses more memory; explicit CPU mode remains available.

## Verification

- `cargo test --locked`: 49 passed, 0 failed, 4 ignored. Native console download, Save as, explicit names, Escape cleanup, wheel input, and maximum zoom run through production UI paths.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo build --locked --release`: passed. Help reports GPU as the default.
- `sh -n scripts/headless_wayland.sh`: passed.
- `scripts/headless_wayland.sh cargo test --locked clipboard::tests::session_clipboard_round_trip -- --ignored --exact`: passed.
- `EXPLORER_CAPTURE_DIR=/tmp/explorer-perf/caps scripts/headless_wayland.sh cargo test --locked screenshot::native_tests::native_clipboard_paste_into_folder -- --ignored --exact`: passed. Real PNG clipboard paste, duplicate naming, text, URL download, and the user's exact public URL were verified by saved bytes. The public image was saved through Save as and checked for RIFF/WEBP content.
- App-owned captures `08-max-zoom.png` and `clipboard-save-as.png` inspected: artwork stays within the pane and the Save as field is visible.

Touched code is in CLI, benchmark, renderer, clipboard/import domain, native screenshots/tests, UI commands/events/overlays/viewport, stylesheet, and the private headless harness. README documents the changed behavior and measurement limits. No dependency changes. All handwritten source files remain below 1000 lines. Per-file quality analysis reports maximum CC 10 for commands/imports/clipboard and lower values for the changed event and benchmark code. CLI parsing retains its existing CC 11 / cognitive 19 function; the change there only selects the renderer default and updates help. Rust MI and CRAP were not collected. The quality stop hook cannot compare Git changes in this non-Git directory; compiler, clippy, tests, and per-file analysis provide the applicable evidence.

Remaining limits: live desktop FPS was deliberately not checked; real trash and Tumbler ignored integration tests were not needed for this change. No desktop input or restart is needed to use the next launch's build. Original source rollback remains in `/tmp/explorer-before`; diagnostic logs and captures remain in `/tmp/explorer-perf`.

Later clipboard, DOM, and dependency patch work is recorded in [clipboard-crashes.md](clipboard-crashes.md).
