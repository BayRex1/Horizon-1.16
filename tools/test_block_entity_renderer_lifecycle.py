#!/usr/bin/env python3
"""Smoke test world entry and renderer replacement after F3+T pack reload.

Run inside a graphical session (for example, xvfb-run -a) with prepared
resources and prebuilt horizon-client/horizon-ctl binaries. A temporary game
directory keeps saves, options, mods and pack caches out of the checkout.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client", type=Path, required=True)
    parser.add_argument("--ctl", type=Path, required=True)
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--dev-mod", type=Path, action="append", default=[])
    parser.add_argument("--failed-reload", action="store_true", help="Inject malformed renderer models, verify rollback and retry")
    args = parser.parse_args()
    client = args.client.resolve()
    ctl = args.ctl.resolve()
    resources = args.resources.resolve()

    with tempfile.TemporaryDirectory(prefix="hzbe-") as folder:
        cwd = Path(folder)
        socket = cwd / "control.sock"
        output = cwd / "client.log"
        if args.failed_reload:
            # Link large immutable resource trees; keep only the model document
            # private so corrupting it cannot affect other running checkouts.
            resource_copy = cwd / "resources"
            resource_copy.mkdir()
            for entry in resources.iterdir():
                destination = resource_copy / entry.name
                if entry.name == "entity-models.json":
                    shutil.copy2(entry, destination)
                else:
                    destination.symlink_to(entry, target_is_directory=entry.is_dir())
            resources = resource_copy

        def request(*words, timeout=15):
            reply = subprocess.run(
                [str(ctl), "--socket", str(socket), "--timeout", str(timeout), *words],
                cwd=cwd, capture_output=True, text=True, timeout=timeout + 5)
            if reply.returncode:
                raise RuntimeError(f"control {' '.join(words)}: {reply.stdout}{reply.stderr}")
            try:
                return json.loads(reply.stdout)
            except json.JSONDecodeError:
                return reply.stdout.strip()

        cmd = [str(client), "--resources", str(resources), "--world", "BlockEntityLifecycle",
               "--test-terrain", "--render-distance", "4", "--offscreen",
               "--control-socket", str(socket), "--timeout-seconds", "180", "--no-pause"]
        for path in args.dev_mod:
            cmd.extend(["--dev-mod", str(path.resolve())])

        with output.open("w+") as log:
            game = subprocess.Popen(cmd, cwd=cwd, stdout=log, stderr=subprocess.STDOUT,
                                    env=os.environ.copy())
            try:
                deadline = time.monotonic() + 120
                while time.monotonic() < deadline:
                    if game.poll() is not None:
                        raise RuntimeError("client exited before control was ready")
                    if socket.exists():
                        try:
                            request("status", timeout=2)
                            break
                        except (OSError, RuntimeError, subprocess.TimeoutExpired, ValueError):
                            pass
                    time.sleep(0.5)
                else:
                    raise TimeoutError("client control socket never became ready")

                request("wait-ready", "120", timeout=130)
                before = request("status")
                def reload():
                    request("key", "f3", "down", timeout=60)
                    try:
                        request("key", "t", "tap", timeout=120)
                    finally:
                        request("key", "f3", "up", timeout=60)
                    request("wait", "5", timeout=30)

                if args.failed_reload:
                    document = resources / "entity-models.json"
                    original = document.read_bytes()
                    try:
                        document.write_bytes(b"{malformed")
                        reload()
                        failure = request("status")
                        if not failure:
                            raise AssertionError("world lost control after a failed reload")
                    finally:
                        document.write_bytes(original)
                reload()
                after = request("status")
                if game.poll() is not None:
                    raise RuntimeError("client exited after pack reload")
                subprocess.run([str(ctl), "--socket", str(socket), "--timeout", "60", "quit"],
                               cwd=cwd, capture_output=True, text=True, timeout=65)
                game.wait(timeout=120)
                if game.returncode:
                    raise RuntimeError(f"client exited with status {game.returncode}")
                log.flush()
                log.seek(0)
                text = log.read()
                for marker in ("block entity renderer ready in", "reloaded block entity renderer in"):
                    if marker not in text:
                        raise AssertionError(f"expected log marker missing: {marker}")
                if "resource reload failed" in text or "block entity renderer failed" in text:
                    if not args.failed_reload or "resource reload failed" not in text:
                        raise AssertionError("unexpected resource failure")
                if args.failed_reload and "resource reload failed" not in text:
                    raise AssertionError("corrupt model reload was not rejected")
                print(json.dumps({"status": "passed", "world_before": bool(before),
                                  "world_after": bool(after),
                                  "loader_logs": [line for line in text.splitlines()
                                                  if "block entity renderer" in line.lower()]},
                                 indent=2))
            except Exception:
                log.flush()
                log.seek(0)
                print(log.read()[-8000:], file=sys.stderr)
                raise
            finally:
                if game.poll() is None:
                    game.terminate()
                    try:
                        game.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        game.kill()
                        game.wait()


if __name__ == "__main__":
    main()
