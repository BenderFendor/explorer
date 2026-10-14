# Dependency fixes

Cargo uses local copies of the locked packages below. Versions and public APIs stay the same. `fixes.patch` records changes from their published sources; each directory includes its upstream MIT and Apache licenses.

| Package | Upstream repository | Release source commit |
| --- | --- | --- |
| blitz-dom 0.2.4 | https://github.com/DioxusLabs/blitz | `0a77560e59440d3c6b5f14b673b9c722a08a3d4c` |
| blitz-shell 0.2.3 | https://github.com/DioxusLabs/blitz | `20ee2f24b4751fa8b072b6f131b56d089c83467d` |
| accesskit_unix 0.13.1 | https://github.com/AccessKit/accesskit | `405d578cdfd8496ee79020d24cfaac8c29a8a48f` |

- Blitz clears hover, focus, active, and mouse-down handles before removing nodes, including generated layout nodes. A later pointer event cannot traverse an interaction handle into a removed slab entry.
- Layout reconstruction and DOM removal release old generated wrappers. Real DOM children and pseudo-elements survive reconstruction; repeated layout keeps node storage bounded.
- Inline hit testing tolerates a node whose text layout is unavailable. It still tests children and the node's box.
- Clipboard initialization returns the existing clipboard error instead of panicking. The root package enables arboard's Wayland backend for native text fields.
- Resource callbacks discard delivery failures after the event loop closes.
- AccessKit reports an unavailable accessibility bus without panicking. A failed AT-SPI connection leaves adapters inactive and preserves the status watcher; a later enable event can reconnect. Healthy accessibility remains enabled.

Regressions live in `src/screenshot/regressions.rs`. `scripts/window_regressions.py` exercises the real window and clipboard on a private Wayland compositor. See [the crash trace](../docs/agents/traces/clipboard-crashes.md) for reproduction evidence and limits.

Remove a local patch once an upstream release supplies the fix and these regressions pass against that release. Do not edit Cargo registry sources.
