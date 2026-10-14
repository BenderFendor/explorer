"""Exercise paste, imports, remounts, and pointer moves on a private Wayland socket."""

import argparse
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from PIL import Image


def key(*arguments):
    # A virtual keyboard needs time to enter the focused surface after its keymap arrives.
    subprocess.run(["wtype", "-s", "1000", *arguments], check=True, timeout=10)


def wait_for(predicate, process):
    deadline = time.monotonic() + 10
    while not predicate():
        assert process.poll() is None, "Explorer exited; inspect window.log"
        assert time.monotonic() < deadline, "Timed out waiting for Explorer"
        time.sleep(0.05)


def memory(process):
    values = Path(f"/proc/{process.pid}/status").read_text().splitlines()
    return int(next(line for line in values if line.startswith("VmRSS:")).split()[1])


def navigation_memory(process):
    arguments = ["wtype", "-s", "1000", "-d", "5"]
    for _ in range(32):
        arguments.extend(["jjkk+-", "-s", "1000"])
    keyboard = subprocess.Popen(arguments)
    samples = []
    try:
        while keyboard.poll() is None:
            assert process.poll() is None, "Explorer exited; inspect window.log"
            samples.append(memory(process))
            time.sleep(1)
        assert keyboard.returncode == 0
        samples.append(memory(process))
    finally:
        if keyboard.poll() is None:
            keyboard.terminate()
            keyboard.wait(timeout=10)
    return samples


def run(binary, output, image_only):
    folder = output / "fixture"
    folder.mkdir()
    for index in range(200):
        (folder / f"item-{index:03}.txt").write_text("headless fixture\n")
    samples = []
    with (output / "window.log").open("w") as log:
        process = subprocess.Popen([str(binary.resolve()), str(folder)], stdout=log, stderr=log)
        try:
            time.sleep(3)
            key("-d", "100", ":touch typed.txt", "-k", "Return")
            wait_for(lambda: (folder / "typed.txt").exists(), process)
            if not image_only:
                subprocess.run(["wl-copy", "--type", "text/plain"], input=b"touch pasted.txt", check=True)
                key(":", "-s", "100", "-M", "ctrl", "-k", "v", "-m", "ctrl", "-k", "Return")
                wait_for(lambda: (folder / "pasted.txt").exists(), process)
            image = io.BytesIO()
            Image.new("RGB", (128, 192), (190, 120, 65)).save(image, format="PNG")
            subprocess.run(["wl-copy", "--type", "image/png"], input=image.getvalue(), check=True)
            for index in range(32):
                subprocess.run(["swaymsg", "seat", "seat0", "cursor", "set", "600", "300"],
                               check=True, stdout=subprocess.DEVNULL)
                key("-M", "ctrl", "-k", "v", "-m", "ctrl")
                wait_for(lambda: len(list(folder.glob("clipboard*.png"))) == index + 1, process)
                key("-d", "5", "jjkk+-")
                samples.append(memory(process))
            assert all(path.read_bytes() == image.getvalue() for path in folder.glob("clipboard*.png"))
            navigation_samples = navigation_memory(process)
            subprocess.run(["swaymsg", '[app_id="explorer"]', "kill"], check=True,
                           stdout=subprocess.DEVNULL)
            process.wait(timeout=10)
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
    assert process.returncode == 0, (output / "window.log").read_text()
    assert "panicked" not in (output / "window.log").read_text()
    report = {"image_pastes": 32, "typed_and_pasted_commands": not image_only,
              "rss_kib": samples, "rss_growth_after_warmup_kib": samples[-1] - samples[4],
              "navigation_rss_kib": navigation_samples,
              "navigation_growth_after_warmup_kib": navigation_samples[-1] - navigation_samples[4]}
    (output / "result.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report))


def main():
    runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    assert runtime.name.startswith("explorer-headless."), "Run through scripts/headless_wayland.sh"
    assert Path(os.environ["SWAYSOCK"]).parent == runtime
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--image-only", action="store_true")
    args = parser.parse_args()
    output = args.output or Path(tempfile.mkdtemp(prefix="explorer-window-qa-"))
    output.mkdir(exist_ok=True)
    print(f"Evidence: {output}", flush=True)
    run(args.binary, output, args.image_only)


if __name__ == "__main__":
    main()
