#!/usr/bin/env python3
"""Profile the block-entity loader and optionally the first title-menu frame.

Build first: cargo build --release -p horizon-client --example block_entity_load_profile
Run from the repo root with prepared resources available under --resources.
"""
import argparse
import json
import re
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/block_entity_load_profile"))
    parser.add_argument("--baseline-binary", type=Path,
                        help="Original-checkout loader profile for paired alternating measurements")
    parser.add_argument("--resources", type=Path, default=Path(".local/horizon-client/26.3"))
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--client-binary", type=Path, help="Client executable to measure through its first title frame")
    parser.add_argument("--baseline-client-binary", type=Path, help="Pre-change client executable for paired comparison")
    parser.add_argument("--client-repeats", type=int, default=3)
    parser.add_argument("--client-cwd", type=Path, help="Game directory (isolated from other client runs)")
    args = parser.parse_args()
    if args.repeats < 0 or (args.repeats == 0 and not args.client_binary):
        parser.error("--repeats must be positive unless running a client benchmark")
    if args.client_repeats < 1:
        parser.error("--client-repeats must be at least one")
    samples = {"optimized": [], "baseline": []}
    def run_loader(name, binary):
        run = subprocess.run([str(binary.resolve()), str(args.resources.resolve())],
                             text=True, capture_output=True, check=True)
        values = {key: float(value) for key, value in
                  re.findall(r"^(registry_ms|block_entity_renderer_ms)=([\d.]+)$", run.stderr, re.M)}
        if len(values) != 2:
            raise RuntimeError(f"incomplete {name} profile output: {run.stderr}")
        samples[name].append(values)
    for index in range(args.repeats):
        order = [("baseline", args.baseline_binary), ("optimized", args.binary)]
        if index % 2:
            order.reverse()
        for name, binary in order:
            if binary:
                run_loader(name, binary)
    result = {"scope": "block-entity loader only; excludes full client startup and first frame",
              "binary": str(args.binary), "resources": str(args.resources), "repeats": args.repeats,
              "metrics": {name: {key: {"median_ms": round(statistics.median(row[key] for row in rows), 3),
                                         "samples_ms": [row[key] for row in rows],
                                         "range_ms": [min(row[key] for row in rows), max(row[key] for row in rows)]}
                                  for key in rows[0]}
                          for name, rows in samples.items() if rows}}
    if samples["baseline"]:
        differences = [round(after["block_entity_renderer_ms"] - before["block_entity_renderer_ms"], 3)
                       for after, before in zip(samples["optimized"], samples["baseline"])]
        result["paired_renderer_ms"] = {
            "median_ms": round(statistics.median(differences), 3),
            "samples_ms": differences,
            "range_ms": [min(differences), max(differences)],
        }
    if args.client_binary:
        if not args.client_cwd:
            parser.error("--client-cwd is required with --client-binary")
        times = {"optimized": [], "baseline": []}
        game_to_frame = {"optimized": [], "baseline": []}
        adapters = set()
        def run_client(name, binary):
            # Every variant starts from the same settings and empty pack cache.
            # This prevents earlier runs from changing subsequent startup work.
            with tempfile.TemporaryDirectory(prefix=f"startup-{name}-",
                                             dir=args.client_cwd.resolve()) as scratch:
                for filename in ("options.txt", "horizon-player.txt"):
                    source = args.client_cwd / filename
                    if source.is_file():
                        shutil.copy2(source, Path(scratch) / filename)
                started = time.perf_counter()
                run = subprocess.run([str(binary.resolve()), "--resources",
                                      str(args.resources.resolve()), "--frames", "1", "--offscreen"],
                                     cwd=scratch, capture_output=True, text=True)
                wall_ms = round((time.perf_counter() - started) * 1000, 3)
            if run.returncode:
                raise RuntimeError(f"{name} client returned {run.returncode}: {run.stderr[-3000:]}")
            marker = re.search(r"Horizon: first frame after (\d+) ms", run.stderr)
            if not marker:
                raise RuntimeError(f"{name} client exited without a first-frame marker: {run.stderr[-2000:]}")
            adapter = re.search(r"Horizon renderer: ([^\n]+)", run.stderr)
            if adapter:
                adapters.add(adapter.group(1))
            times[name].append(wall_ms)
            game_to_frame[name].append(int(marker.group(1)))
        for index in range(args.client_repeats):
            # Alternate run order to reduce systematic warm-cache and host-load bias.
            order = [("baseline", args.baseline_client_binary), ("optimized", args.client_binary)]
            if index % 2:
                order.reverse()
            for name, binary in order:
                if binary:
                    run_client(name, binary)
        result["title_first_frame"] = {
            "scope": "wall time from process launch through the first offscreen title frame submission and process shutdown; GPU completion is not confirmed. Also record the separate client timer starting at Game::new",
            "measurements": {name: {"median_ms": round(statistics.median(values), 3),
                                    "samples_ms": values, "range_ms": [min(values), max(values)]}
                             for name, values in times.items() if values},
            "client_binary": str(args.client_binary),
            "baseline_client_binary": str(args.baseline_client_binary) if args.baseline_client_binary else None,
            "game_dir": str(args.client_cwd),
            "fresh_game_directory_per_run": True,
            "gpu_adapters": sorted(adapters),
            "game_constructed_to_first_frame_ms": {
                name: {"median_ms": statistics.median(values), "samples_ms": values}
                for name, values in game_to_frame.items() if values
            },
        }
        if times["baseline"]:
            differences = [round(after - before, 3)
                           for after, before in zip(times["optimized"], times["baseline"])]
            result["title_first_frame"]["paired_optimized_minus_baseline_ms"] = {
                "median_ms": round(statistics.median(differences), 3),
                "samples_ms": differences,
                "range_ms": [min(differences), max(differences)],
            }
    print(json.dumps(result, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
