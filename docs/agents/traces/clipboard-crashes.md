# Clipboard and DOM crashes

Goal: reproduce the reported failures, repair their shared causes, verify clipboard text and image paste in a private Wayland window, then replace the installed Explorer. Keep GPU as the approved default. Never send input to the user's desktop.

Status: reproduced clipboard and DOM failures are fixed, verified, documented, and installed. A confirmed generated-wrapper leak is fixed. The exact OS allocation failure and remaining RSS growth are unresolved; do not claim they are eliminated.

## Evidence and cause

- The installed baseline reproduces the reported AccessKit `ConnectionRefused` panic in private Wayland. Evidence: `/tmp/explorer-crash/before.log`. The accessibility bus is unavailable; the background thread unwrapped its connection error.
- Removing a hovered node and moving the pointer reproduces `blitz-dom/src/traversal.rs:227: invalid key` through the real Dioxus document. An interaction handle retained the removed node's numeric slab ID. Removal must clear those handles while nodes and their layout ancestors still exist.
- Removing an inline node's computed text layout reproduces the reported `Option::unwrap()` panic in hit testing. Evidence: `/tmp/explorer-crash/inline-baseline.log`. This injects the invalid state; the application's original sequence that creates it is unconfirmed. Hit testing now skips unavailable text layout and preserves box hit testing.
- The installed baseline can type `:touch typed.txt`, but pasting a command crashes in Blitz's clipboard initialization when X11 is unavailable. Evidence: `/tmp/explorer-crash/baseline-window/window.log`. Native text fields used arboard without its Wayland feature, while folder paste used wl-clipboard-rs. Enable the installed dependency's Wayland backend and propagate its existing error type.
- The installed baseline saved 32 PNG clipboard images without a main-thread crash. Its final check failed on the independently reproduced AccessKit panic. An image paste alone does not reproduce the reported allocation failure.
- The first patched release passed 32 byte-exact image pastes and text commands without panics. RSS rose from 255 to 286 MiB. Repeating zoom/navigation on an unchanged folder also grew RSS, including with one persistent virtual keyboard. This ruled out clipboard contents and per-command virtual keyboard creation as the source of that growth.
- A minimal unchanged Dioxus document retained one generated layout wrapper per resolve: 14 nodes became 46 after 32 passes. Evidence: `/tmp/explorer-crash/layout-growth-baseline.log`. Reconstructed layout replaced its child list without dropping previous anonymous wrappers. Cleanup now removes old wrappers before reconstruction and DOM removal, preserves pseudo-elements and actual DOM children, and repairs children's layout-parent handles. The regression checks stable storage, preserved focus and pseudo-elements, and safe pointer movement. This is a confirmed leak; the specific OS allocation failure remains unreproduced.
- The `EventLoopClosed(ResourceLoad...)` panic is a secondary failure: asynchronous image delivery unwraps an error after the main event loop has exited. Ignore delivery failures during shutdown.
- `Io error: Cannot allocate memory` was not reproduced. The same error prefix is emitted by the Wayland backend. At a read-only host check, available RAM was about 9 GB, swap already used about 17 GB, and committed address space exceeded CommitLimit under `vm.overcommit_memory=0`. These observations suggest investigating host pressure; they do not establish the cause of the reported syscall failure. No other processes or host settings were changed.

## Changes

- `Cargo.toml` and `Cargo.lock`: unify arboard's `wayland-data-control` feature and apply local patches to the existing locked Blitz and AccessKit versions.
- `vendor/`: exact published dependency sources, licenses, provenance, and a patch manifest. Release old generated layout wrappers; clear interaction handles on all three node-removal paths; tolerate missing inline text layout; return clipboard errors; handle closed event loops and unavailable accessibility services.
- `src/screenshot/regressions.rs` and its module declaration: deterministic DOM regressions and an ignored real Wayland caret/selection paste test.
- `scripts/headless_wayland.sh`: also unset DISPLAY so tests cannot use the desktop's X11 clipboard.
- `scripts/window_regressions.py`: assert a private compositor socket; exercise typed and pasted commands, 32 byte-exact PNG imports, pointer movement, navigation, zoom, clean shutdown, and RSS samples.
- `README.md`: clipboard field behavior and regression commands.

Temporary keyboard logging was removed. No registry sources, desktop input, existing files, or global allocator/overcommit settings were modified.

## Verification

- `cargo test --locked`: 52 passed, 0 failed, 5 ignored. The affected native UI suite passes.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `scripts/headless_wayland.sh cargo test --locked screenshot::regressions::native_editor_pastes_wayland_text_at_caret -- --ignored --exact`: passed. Paste inserts a URL after `dl `; Ctrl+A and paste replaces the selection.
- All three deterministic regressions pass with the patches, including bounded layout-node storage. Restoring the old inline-layout unwrap makes that regression fail at the reported unwrap.

First patched release: real window clipboard stress passed, clean exit and no panics. RSS samples are in `/tmp/explorer-crash/final-window/result.json`. Native clipboard folder test also passed: PNG copies, text, URL download, the exact user-supplied public image URL, and Save as. Final GPU benchmark before wrapper cleanup: mean 5.23 ms, p95 6.53 ms, wheel first movement/settle 7.64 ms, idle redraws zero, RSS 255 MB. These measure app processing, not desktop FPS.

Memory comparison evidence: `/tmp/explorer-crash/constant-folder.json` and `/tmp/explorer-crash/single-keyboard.json`. Final wrapper-fixed release passed the real-window test: 32 byte-exact PNG imports, typed and pasted commands, repeated pointer/navigation/zoom, no panics, and clean exit. Evidence: `/tmp/explorer-crash/final-wrapper-fix-rerun/result.json`. RSS still grew 23,540 KiB after import warm-up and 29,804 KiB during navigation after warm-up. The CPU navigation comparison grew 8,896 KiB after warm-up (`/tmp/explorer-crash/cpu-navigation.json`); this is a different renderer on the same folder, not proof of a specific GPU leak. Vello resets its scene after each render. Further attribution needs allocation profiling; RSS alone does not distinguish live allocations from allocator/driver retention.

Final off-screen GPU measurement: mean 6.29 ms, p95 7.99 ms, wheel first movement/settle 8.46 ms, zero idle redraws, RSS 291 MB (`/tmp/explorer-crash/final-wrapper-gpu-bench.json`). Timing varies from the earlier 5.23 ms sample; no speedup from wrapper cleanup is claimed.

`cargo build --locked --release`, root and patched-source formatting, Python compilation, and shell syntax checks passed. Both ignored clipboard tests passed again after wrapper cleanup.

Installed `/home/bender/.local/bin/explorer` matches the release binary: SHA256 `a7f1b9b01259fd01710995c0d10a6a74f6038eafed684fad1cf0630f325e4c4a`. The immediately preceding executable is backed up at `/tmp/explorer-before-wrapper-fix-jg2b70jo/explorer`.

## Harness friction

`wtype` can send initial characters before its virtual keyboard enters the surface. An empty folder after a typed command was a harness failure, with Explorer still alive. A one-second keyboard-enter delay and 100 ms typed-character delay reliably delivered the first command in separate probes. Use the regression script instead of assuming early virtual input was received.

## Limits and rollback

The operating-system allocation failure remains unconfirmed. The missing inline-layout state is reproduced directly, but its original application trigger is unconfirmed. Healthy desktop AT-SPI integration and real compositor FPS were not tested. GPU mode retains the previously approved memory tradeoff.

The old installed executable is backed up at `/tmp/explorer-before-clipboard-fix-kdc45b1x/explorer`; original sources remain at `/tmp/explorer-before`. The previous installed SHA256 is `6d5d6c9b3f0c7edcd00a7f3fa8f6afb6c066fe33ce7809afdce068c274e74dc0`. The first patched release SHA256 is `595ae1520decf9b84cd0714a5c524cfb484f897864a5f155eb73e2c3d324d072`. Replacing the executable affects the next launch; existing windows are not restarted.
