"""Measure memory and CPU for each renderer in a real Hyprland window.

Usage: python scripts/measure_renderers.py [FOLDER] [--binary PATH]
Needs hyprctl, ydotool with ydotoold running, and the release binary.
"""

import argparse
import json
import os
import subprocess
import time
from pathlib import Path

TICKS = os.sysconf("SC_CLK_TCK")
os.environ.setdefault("YDOTOOL_SOCKET", f"/run/user/{os.getuid()}/.ydotool_socket")


def cpu_seconds(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / TICKS


def memory(pid):
    rollup = Path(f"/proc/{pid}/smaps_rollup").read_text()
    values = {line.split(":")[0]: int(line.split()[1]) for line in rollup.splitlines()[1:]}
    return values["Rss"] / 1024, values["Pss"] / 1024


def window():
    for client in json.loads(subprocess.check_output(["hyprctl", "clients", "-j"])):
        if client["class"] == "explorer":
            return client["at"], client["size"]
    raise SystemExit("explorer window not found")


def usage(pid, seconds, action=None):
    start, began = cpu_seconds(pid), time.monotonic()
    while time.monotonic() - began < seconds:
        if action:
            action()
        else:
            time.sleep(0.1)
    return 100 * (cpu_seconds(pid) - start) / (time.monotonic() - began)


def wheel(direction):
    def step():
        subprocess.run(["ydotool", "mousemove", "--wheel", "-x", "0", "-y", str(direction)],
                       stdout=subprocess.DEVNULL, check=True)
        time.sleep(0.03)
    return step


def measure(binary, folder, renderer):
    process = subprocess.Popen([binary, f"--renderer={renderer}", folder],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        time.sleep(6)
        (x, y), (w, h) = window()
        subprocess.run(["hyprctl", "dispatch", f"hl.dsp.cursor.move({{x={x + w // 2 + 80},y={y + h // 2}}})"],
                       stdout=subprocess.DEVNULL, check=True)
        idle = usage(process.pid, 4)
        down = usage(process.pid, 4, wheel(-1))
        up = usage(process.pid, 4, wheel(1))
        rss, pss = memory(process.pid)
        return {"renderer": renderer, "window": f"{w}x{h}", "rss_mb": round(rss), "pss_mb": round(pss),
                "idle_cpu_pct": round(idle, 1), "scroll_cpu_pct": round((down + up) / 2, 1)}
    finally:
        process.terminate()
        process.wait()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("folder", nargs="?", default=str(Path.home() / "Pictures"))
    parser.add_argument("--binary", default=str(Path(__file__).resolve().parents[1] / "target/release/explorer"))
    parser.add_argument("--renderers", default="cpu,gpu")
    options = parser.parse_args()
    for renderer in options.renderers.split(","):
        print(json.dumps(measure(options.binary, options.folder, renderer)))


if __name__ == "__main__":
    main()
