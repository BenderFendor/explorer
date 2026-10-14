"""Verify Explorer's own --screenshot path with isolated folders and Matugen colors."""

import hashlib
import io
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from PIL import Image


def capture(binary, folder, destination, environment):
    result = subprocess.run(
        [str(binary), str(folder), f"--screenshot={destination}"],
        env=environment, capture_output=True, text=True, timeout=30, check=True,
    )
    print(result.stdout.strip())
    with Image.open(destination) as screenshot:
        assert screenshot.size == (1120, 740)
        assert len(screenshot.getcolors(1120 * 740)) > 100
        analysis = io.BytesIO()
        screenshot.resize((3360, 2220), Image.Resampling.LANCZOS).save(analysis, format="PNG")
    return subprocess.check_output(
        ["tesseract", "stdin", "stdout", "--psm", "11"],
        input=analysis.getvalue(), stderr=subprocess.DEVNULL,
    ).decode()


def make_fixture(fixture):
    folder = fixture / "Studio"
    folder.mkdir()
    for name in ["Downloads", "Music", "Pictures", "Projects", "Work"]:
        (folder / name).mkdir()
    for name in ["notes.txt", "project.toml", ".private"]:
        (folder / name).write_text("File explorer check\n")
    preview = folder / "reference.svg"
    preview.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="128" height="96"><rect width="128" height="96" fill="#d8d9e6"/><circle cx="64" cy="48" r="26" fill="#787aa8"/></svg>')
    return folder, preview


def make_config(fixture):
    source = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    config = fixture / "config"
    (config / "matugen").mkdir(parents=True)
    palette = config / "matugen/colors.css"
    shutil.copyfile(source / "matugen/colors.css", palette)
    user_dirs = source / "user-dirs.dirs"
    if user_dirs.exists():
        shutil.copyfile(user_dirs, config / "user-dirs.dirs")
    return config, palette


def check_palette(binary, folder, palette, destination, environment):
    original = palette.read_text()
    palette.write_text(re.sub(r"(@define-color primary_fixed_dim )#[0-9a-fA-F]+", r"\g<1>#cc6f23", original))
    capture(binary, folder, destination, environment)
    with Image.open(destination) as screenshot:
        assert sum(count for count, pixel in screenshot.getcolors(1120 * 740) if pixel[:3] == (204, 111, 35)) > 20


def main():
    binary = Path(__file__).resolve().parents[1] / "target/debug/explorer"
    output = Path(tempfile.mkdtemp(prefix="explorer-native-qa-"))
    with tempfile.TemporaryDirectory(prefix="explorer-fixture-") as temporary:
        fixture = Path(temporary)
        folder, preview = make_fixture(fixture)
        config, palette = make_config(fixture)
        environment = dict(os.environ, XDG_CONFIG_HOME=str(config), WAYLAND_DISPLAY="", DISPLAY="")
        try:
            text = capture(binary, folder, output / "matugen.png", environment)
            assert re.search(r"[8B]\s*items", text), text
            assert "Projects" in text and "notes.txt" in text and "reference.svg" in text, text
            subprocess.run([str(binary), str(folder), "--screenshot"], cwd=output, env=environment, capture_output=True, text=True, timeout=30, check=True)
            assert (output / "explorer-screenshot.png").is_file()
            check_palette(binary, folder, palette, output / "palette-update.png", environment)
            text = capture(binary, folder / "missing", output / "error.png", environment)
            assert re.search(r"open this folder", text), text
            result = subprocess.run([str(binary), str(folder), f"--screenshot={output / 'matugen.png'}"], env=environment, capture_output=True, text=True, timeout=30)
            assert result.returncode != 0 and "File exists" in result.stderr
        finally:
            cache = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "thumbnails/large"
            (cache / (hashlib.md5(preview.as_uri().encode()).hexdigest() + ".png")).unlink(missing_ok=True)
    print(f"App-owned screenshots, Matugen colors, folder errors, and output protection passed. Captures: {output}")


if __name__ == "__main__":
    main()
