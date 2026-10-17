#!/usr/bin/env python3
"""Profile Horizon title-screen startup on two client revisions.

Both binaries must be built with the same profile/target/toolchain, and contain
the HORIZON_STARTUP_PROFILE timing markers (assets.rs and game.rs).

Example:
    python3 tools/profile_client_startup.py \
        --baseline /tmp/horizon-original --optimized /tmp/horizon-prepared \
        --resources .local/horizon-client/26.3 --runs 3 \
        --output benchmarks/block-facts-startup.json

The client is launched to its first rendered frame, then exits via --frames 1.
Cold samples issue best-effort POSIX_FADV_DONTNEED for relevant input files
before each process; this is not an OS-wide cache drop. Warm samples follow
without cache eviction. Both variants share one display and identical options.
Each raw measurement is retained, including errors.
"""
import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

PHASE = re.compile(r"HORIZON_STARTUP_PHASE ([a-z_]+) ([0-9.]+)")
PHASES = ("resource_root", "registry", "assets", "block_facts",
          "resource_tables", "game_constructed", "first_frame")
RESOURCE_FILES = (
    "horizon-resources.json", "facts.json", "block-tags.json",
    "gameplay-facts.json", "block-behaviours.json", "gameplay-facts-prepared.bin",
    "render-definitions.json", "model-states.json",
)


def evict(path):
    """Best-effort advisory eviction. Returns True only if the syscall worked."""
    if not hasattr(os, "posix_fadvise"):
        return False
    try:
        with open(path, "rb") as source:
            os.posix_fadvise(source.fileno(), 0, 0, os.POSIX_FADV_DONTNEED)
        return True
    except (OSError, ValueError):
        return False


def display_server():
    """Start one Xvfb for all compared samples, if DISPLAY is not already set."""
    if os.environ.get("DISPLAY"):
        return None, os.environ["DISPLAY"]
    if shutil.which("Xvfb") is None:
        raise RuntimeError("No DISPLAY and Xvfb is not installed")
    for number in range(81, 120):
        if Path(f"/tmp/.X11-unix/X{number}").exists():
            continue
        proc = subprocess.Popen(
            ["Xvfb", f":{number}", "-screen", "0", "1280x800x24", "-nolisten", "tcp"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        time.sleep(0.15)
        if proc.poll() is None:
            return proc, f":{number}"
    raise RuntimeError("Could not launch Xvfb")


def profile_one(binary, root, cwd, display, mode, timeout):
    if mode == "cold":
        for name in RESOURCE_FILES:
            candidate = root / name
            if candidate.is_file():
                evict(candidate)
        evict(binary)
    env = dict(os.environ, DISPLAY=display, HORIZON_STARTUP_PROFILE="1",
               HORIZON_MENU_STYLE="classic", LIBGL_ALWAYS_SOFTWARE="1")
    command = [str(binary), "--resources", str(root), "--frames", "1",
               "--timeout-seconds", str(timeout), "--offscreen"]
    start = time.perf_counter()
    try:
        run = subprocess.run(command, cwd=cwd, env=env, text=True,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             timeout=timeout + 15)
        wall = (time.perf_counter() - start) * 1000
        log = run.stderr + "\n" + run.stdout
        phases = {name: float(ms) for name, ms in PHASE.findall(log)}
        if run.returncode or any(phase not in phases for phase in PHASES):
            raise RuntimeError(
                f"exit={run.returncode}; phases={phases}; "
                f"last log lines:\n" + "\n".join(log.splitlines()[-25:]))
        times = {
            "registry_ms": phases["registry"],
            "resource_root_ms": phases["resource_root"],
            "assets_ms": phases["assets"] - phases["resource_root"],
            "block_facts_ms": phases["block_facts"] - phases["assets"],
            "remaining_resource_ms": phases["resource_tables"] - phases["block_facts"],
            "construct_ms": phases["game_constructed"] - phases["resource_tables"],
            "first_frame_ms": phases["first_frame"],
            "process_wall_ms": wall,
        }
        return {"ok": True, "phases_ms": phases, "times_ms": times}
    except (subprocess.TimeoutExpired, RuntimeError) as exc:
        return {"ok": False, "error": str(exc)}


def median(rows, key):
    values = [r["times_ms"][key] for r in rows if r["ok"]]
    return round(statistics.median(values), 3) if values else None


def digest(path):
    hasher = hashlib.sha256()
    with open(path, "rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            hasher.update(block)
    return hasher.hexdigest()


def measure_preparation(exe, root, output):
    """Time cold generation and an unchanged preparation with an isolated copy."""
    with tempfile.TemporaryDirectory(prefix="horizon-block-facts-preparation-") as temp:
        folder = Path(temp)
        for name in ("facts.json", "gameplay-facts.json", "block-behaviours.json"):
            shutil.copy2(root / name, folder / name)
        samples = {}
        for mode in ("generate", "unchanged"):
            started = time.perf_counter()
            run = subprocess.run([str(exe), str(folder), "prepare"],
                                 capture_output=True, text=True, timeout=180)
            wall = (time.perf_counter() - started) * 1000
            if run.returncode:
                raise RuntimeError("Resource preparation failed: " + run.stderr)
            match = re.search(r"registry_setup_ms=([0-9.]+).*?prepare_ms=([0-9.]+)",
                              run.stdout, re.S)
            if not match:
                raise RuntimeError("Missing preparation timings: " + run.stdout)
            samples[mode] = {
                "registry_setup_ms": float(match.group(1)),
                "cache_prepare_ms": float(match.group(2)),
                "process_wall_ms": round(wall, 3),
            }
        output["resource_preparation"] = samples


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--optimized", type=Path, required=True)
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--prepare-exe", type=Path,
                        help="Optional prepared_block_facts_bench binary for separate preparation cost")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    binaries = {"baseline": args.baseline.resolve(), "optimized": args.optimized.resolve()}
    roots = {"baseline": args.resources.resolve(),
             "optimized": args.resources.resolve()}
    for name, binary in binaries.items():
        if not binary.is_file():
            parser.error(f"{name} binary does not exist: {binary}")
    for name in RESOURCE_FILES[:3]:
        if not (roots["baseline"] / name).is_file():
            parser.error(f"resources are missing {name}")
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    report = {
        "methodology": "two optimized binaries, one Xvfb, --frames 1 --offscreen",
        "cache_cold_definition": "best-effort POSIX_FADV_DONTNEED for listed resources and binary",
        "cache_warm_definition": "no eviction between trials (separate client processes)",
        "binary_paths": {k: str(v) for k, v in binaries.items()},
        "binary_sha256": {k: digest(v) for k, v in binaries.items()},
        "system": {"platform": platform.platform(), "python": platform.python_version()},
        "source_sha256": {name: digest(roots["baseline"] / name)
                          for name in ("facts.json", "gameplay-facts.json", "block-behaviours.json")},
        "resources": str(roots["baseline"]), "samples": [], "medians_ms": {},
    }
    if args.prepare_exe:
        measure_preparation(args.prepare_exe.resolve(), roots["optimized"], report)
    server, display = display_server()
    try:
        with tempfile.TemporaryDirectory(prefix="horizon-startup-") as scratch:
            cwd = Path(scratch)
            # Always use fresh, identical user settings. No persistent profile,
            # saves, or pack/mod selection can leak between variants.
            (cwd / "options.txt").write_text('resourcePacks:["vanilla"]\n')
            for mode in ("cold", "warm"):
                for n in range(args.runs):
                    # Alternate order, avoiding one variant always running last.
                    order = ("baseline", "optimized") if n % 2 == 0 else ("optimized", "baseline")
                    for label in order:
                        result = profile_one(binaries[label], roots[label], cwd, display,
                                             mode, args.timeout)
                        report["samples"].append({"revision": label, "mode": mode,
                                                  "repeat": n, **result})
                        print(f"{mode} #{n + 1} {label}: " +
                              (f'{result["times_ms"]["first_frame_ms"]:.1f} ms to first frame'
                               if result["ok"] else "FAILED: " + result["error"][-300:]),
                              flush=True)
    finally:
        if server is not None:
            server.terminate()
            try:
                server.wait(timeout=3)
            except subprocess.TimeoutExpired:
                server.kill()
    keys = ("registry_ms", "resource_root_ms", "assets_ms", "block_facts_ms",
            "remaining_resource_ms", "construct_ms", "first_frame_ms", "process_wall_ms")
    for mode in ("cold", "warm"):
        for label in binaries:
            selected = [r for r in report["samples"] if r["mode"] == mode
                        and r["revision"] == label]
            report["medians_ms"][f"{mode}_{label}"] = {
                key: median(selected, key) for key in keys
            }
    output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Report: {output}")
    if any(not sample["ok"] for sample in report["samples"]):
        raise SystemExit("Some startup runs failed; inspect report for full details")


if __name__ == "__main__":
    main()
