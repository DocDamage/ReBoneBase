"""Exercise the actual native window and capture layouts on an isolated X11 display.

Run after cargo build:
    xvfb-run -a -s "-screen 0 1920x1200x24" python scripts/native_workspace_smoke.py
Requires xdotool and ImageMagick's import command. No user configuration is read.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/rebonebase")
    parser.add_argument("--output", default="native-workspace-shots")
    parser.add_argument("--emit-json", action="store_true")
    args = parser.parse_args()
    repository = Path(__file__).resolve().parent.parent
    binary = (repository / args.binary).resolve()
    output = (repository / args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="rebonebase-ui-") as temporary:
        root = Path(temporary)
        environment = os.environ.copy()
        for key in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
            directory = root / key.lower()
            directory.mkdir(mode=0o700)
            environment[key] = str(directory)
        environment["REBONEBASE_HOME"] = str(root / "store")
        environment["LIBGL_ALWAYS_SOFTWARE"] = "1"
        sample = root / "Wave.rebone"
        shutil.copyfile(repository / "examples/output/Wave.rebone", sample)
        original = sample.read_bytes()

        def command(*parts):
            return subprocess.run(parts, check=True, capture_output=True, text=True, env=environment).stdout.strip()

        with (output / "native-app.log").open("w") as log:
            app = subprocess.Popen([str(binary), str(sample)], env=environment, stdout=log, stderr=log)
            try:
                window = None
                deadline = time.monotonic() + 30
                while time.monotonic() < deadline:
                    if app.poll() is not None:
                        raise RuntimeError("Native app exited before creating its window")
                    try:
                        windows = command("xdotool", "search", "--onlyvisible", "--pid", str(app.pid)).splitlines()
                        if windows:
                            window = windows[0]
                            break
                    except subprocess.CalledProcessError:
                        pass
                    time.sleep(0.2)
                if window is None:
                    raise RuntimeError("Native window did not become visible")
                command("xdotool", "windowfocus", window)

                def capture(name, width, height):
                    command("xdotool", "windowsize", window, str(width), str(height), "windowmove", window, "0", "0")
                    time.sleep(0.8)
                    if app.poll() is not None:
                        raise RuntimeError("Native app exited during workspace checks")
                    image = output / name
                    command("import", "-window", window, str(image))
                    if image.stat().st_size < 1000:
                        raise RuntimeError("Empty native screenshot")
                    print(f"Captured {name}: {width}x{height}", flush=True)
                    if args.emit_json:
                        data = base64.b64encode(image.read_bytes()).decode("ascii")
                        chunks = [data[i:i+6000] for i in range(0, len(data), 6000)]
                        for index, chunk in enumerate(chunks):
                            print("REBONE_SCREENSHOT " + json.dumps({
                                "path": name, "index": index, "total": len(chunks), "data": chunk,
                            }), flush=True)

                capture("workspace-1280x800.png", 1280, 800)
                command("xdotool", "key", "--clearmodifiers", "space")
                time.sleep(0.3)
                command("xdotool", "key", "--clearmodifiers", "Home")
                capture("workspace-1000x640.png", 1000, 640)
                command("xdotool", "windowsize", window, "1280", "800")
                command("xdotool", "key", "--clearmodifiers", "ctrl+p")
                time.sleep(0.3)
                command("xdotool", "type", "--clearmodifiers", "workspace")
                command("xdotool", "key", "--clearmodifiers", "Down")
                capture("workspace-commands.png", 1280, 800)
                command("xdotool", "key", "--clearmodifiers", "Escape")
                if sample.read_bytes() != original:
                    raise RuntimeError("View-only checks modified the sample file")
                print("Native launch, resizing, transport shortcuts, palette input, and captures completed.", flush=True)
            finally:
                log.flush()
                print((output / 'native-app.log').read_text(errors='replace'), flush=True)
                app.terminate()
                try:
                    app.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()


if __name__ == "__main__":
    main()
