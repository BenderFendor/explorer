#!/bin/sh
# Runs a command in a private headless sway session, including its clipboard.
# Usage: scripts/headless_wayland.sh COMMAND [ARGS...]
set -eu
runtime=$(mktemp -d "${XDG_RUNTIME_DIR:?}/explorer-headless.XXXXXX")
XDG_RUNTIME_DIR="$runtime" WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
    WAYLAND_DISPLAY= WAYLAND_SOCKET= sway --config /dev/null --unsupported-gpu \
    >"$runtime/sway.log" 2>&1 &
sway_pid=$!
cleanup() {
    kill "$sway_pid" 2>/dev/null || true
    wait "$sway_pid" 2>/dev/null || true
    rm -rf "$runtime"
}
trap cleanup EXIT
display=
for _ in $(seq 50); do
    for socket in "$runtime"/wayland-*; do
        [ -S "$socket" ] && display=${socket##*/} && break
    done
    [ -n "$display" ] && break
    kill -0 "$sway_pid" 2>/dev/null || break
    sleep 0.1
done
if [ -z "$display" ]; then
    cat "$runtime/sway.log" >&2
    exit 1
fi
env -u WAYLAND_SOCKET -u DISPLAY XDG_RUNTIME_DIR="$runtime" WAYLAND_DISPLAY="$display" \
    SWAYSOCK="$runtime/sway-ipc.$(id -u).$sway_pid.sock" "$@"
