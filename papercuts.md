## 2026-10-12 22:01

**What happened:** Dioxus Native 0.7.10 fails to compile without hot-reload

**Probable cause:** Config references optional dioxus_cli_config unconditionally

**Fix or workaround:** Enable the published hot-reload feature until the upstream dependency is fixed

---

## 2026-10-12 22:28

**What happened:** Hyprland 0.56 Lua mode rejects legacy dispatcher commands

**Probable cause:** hyprctl dispatch now takes one hl.dsp expression

**Fix or workaround:** Use the current Lua dispatcher API in the native smoke harness

---

## 2026-10-12 22:28

**What happened:** Tumbler cache timestamps include microseconds

**Probable cause:** Installed Tumbler writes fractional Thumb::MTime rather than integer seconds

**Fix or workaround:** Validate both standard integer timestamps and fractional timestamps at their stored precision

---

## 2026-10-12 22:28

**What happened:** Enabling zbus Tokio breaks AccessKit in Dioxus Native

**Probable cause:** Cargo feature unification selects Tokio inside accessibility threads without a reactor

**Fix or workaround:** Keep the existing zbus async-io backend; await its futures from the app runtime

---

## 2026-10-12 22:47

**What happened:** Inline font-face rules are not fetched by Blitz 0.2

**Probable cause:** Font loading runs through the linked stylesheet loader, not inline style processing

**Fix or workaround:** Bundle original TTFs and load font-face CSS through a data-URL stylesheet

---

## 2026-10-12 22:47

**What happened:** Dioxus tabindex div does not receive Blitz autofocus

**Probable cause:** Blitz computes focusability before Dioxus assigns dynamic attributes

**Fix or workaround:** Use a native button as the navigation focus target and exercise real Blitz key events

---

## 2026-10-13 06:24

**What happened:** Clippy useless_format fires on Dioxus rsx text nodes like "{name}" for a lone String

**Probable cause:** rsx expands a sole interpolation to format!

**Fix or workaround:** Pass the value as a node: span { {name} } instead of span { "{name}" }

---

## [auto-mined] [claude]

**What happened:** Integration tests repeatedly hit rate limits (63x 429, 0x "rate limit") - agent may have iterated on code instead of waiting for cooldown.

**Probable cause:** External API rate limiting not handled in test mode.

**Fix or workaround:** Cache API responses for test runs. Add cooldown detection to test harness.

---

## [auto-mined] [claude]

**What happened:** Process killed (OOM) during compilation.

**Probable cause:** Multiple agents compiling simultaneously (rustc + linker memory spikes).

**Fix or workaround:** Serialize cargo build/test across agents.

---

## [auto-mined] [omp]

**What happened:** Edit tool rejected 2x due to stale file tags (file changed between read and edit).

**Probable cause:** Concurrent agents editing the same file, or sequential edits without re-reading.

**Fix or workaround:** Ensure subagents have disjoint file ownership. Re-read before editing when another agent is active.

---

## [auto-mined] [omp]

**What happened:** Agents collided on shared files (19x references to another agent's changes).

**Probable cause:** Multiple subagents editing the same file or crate simultaneously.

**Fix or workaround:** Assign disjoint file ownership in task specs. Watchdog should detect cross-agent file contention.

---

## [auto-mined] [omp]

**What happened:** Build succeeded but typecheck (`tsc --noEmit`) failed.

**Probable cause:** `next.config.ts` has `ignoreBuildErrors: true`, masking real TypeScript errors.

**Fix or workaround:** Run `npx tsc --noEmit` separately as a verification gate. Do not rely on `npm run build` for type safety.

---

## 2026-10-13 07:35

**What happened:** pkill -f / pgrep -f with the app's own arguments killed the calling shell (exit 144)

**Probable cause:** the shell's command line contains the same pattern

**Fix or workaround:** match by name with pgrep -x explorer and check /proc/PID/cmdline

---

## 2026-10-13 08:04

**What happened:** Headless sway chose wayland-1 in a fresh runtime directory, so a fixed wayland-0 wait timed out

**Probable cause:** wl_display_add_socket_auto does not guarantee socket zero

**Fix or workaround:** Discover socket files only inside the private runtime directory

---

## 2026-10-13 08:25

**What happened:** Background Dioxus file operation stopped when its command field unmounted, leaving a stuck job

**Probable cause:** spawn owns the task in the invoking component scope

**Fix or workaround:** Use dioxus_core::spawn_forever for file jobs that must survive menus and command fields

---

## 2026-10-13 08:32

**What happened:** The legacy renderer measurement script sends input to the live desktop and can select an existing Explorer window

**Probable cause:** It locates windows by app class rather than its child PID and uses ydotool

**Fix or workaround:** Use explorer --bench for performance and scripts/headless_wayland.sh for private window checks

---

## 2026-10-13 09:26

**What happened:** wtype can drop the first characters in a private Wayland test

**Probable cause:** Virtual keyboard keymap arrives before keyboard enter; early events have no focus target

**Fix or workaround:** Wait one second after keymap creation and type the initial command with a 100 ms character delay; use scripts/window_regressions.py

---

