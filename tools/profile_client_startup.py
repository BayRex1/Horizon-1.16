#!/usr/bin/env python3
"""Profile the block-entity loader and optionally the first title-menu frame.

Build first: cargo build --release -p horizon-client --example block_entity_load_profile
Run from the repo root with prepared resources available under --resources.
"""
import argparse
import json
import re
import statistics
import subprocess
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/block_entity_load_profile"))
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
    samples = []
    for _ in range(args.repeats):
        run = subprocess.run([str(args.binary.resolve()), str(args.resources.resolve())],
                             text=True, capture_output=True, check=True)
        values = {key: float(value) for key, value in
                  re.findall(r"^(registry_ms|block_entity_renderer_ms)=([\d.]+)$", run.stderr, re.M)}
        if len(values) != 2:
            raise RuntimeError(f"incomplete profile output: {run.stderr}")
        samples.append(values)
    result = {"scope": "block-entity loader only; excludes full client startup and first frame",
              "binary": str(args.binary), "resources": str(args.resources), "repeats": args.repeats,
              "metrics": {key: {"median_ms": round(statistics.median(row[key] for row in samples), 3),
                                "samples_ms": [row[key] for row in samples]}
                          for key in samples[0]} if samples else {}}
    if args.client_binary:
        if not args.client_cwd:
            parser.error("--client-cwd is required with --client-binary")
        times = {"optimized": [], "baseline": []}
        def run_client(name, binary):
            started = time.perf_counter()
            run = subprocess.run([str(binary.resolve()), "--resources",
                                  str(args.resources.resolve()), "--frames", "1", "--offscreen"],
                                 cwd=args.client_cwd.resolve(), capture_output=True, text=True)
            if run.returncode:
                raise RuntimeError(f"{name} client returned {run.returncode}: {run.stderr[-3000:]}")
            times[name].append(round((time.perf_counter() - started) * 1000, 3))
        for index in range(args.client_repeats):
            # Alternate run order to reduce systematic warm-cache and host-load bias.
            order = [("baseline", args.baseline_client_binary), ("optimized", args.client_binary)]
            if index % 2:
                order.reverse()
            for name, binary in order:
                if binary:
                    run_client(name, binary)
        result["title_first_frame"] = {
            "scope": "wall time process launch to exit after first title-menu frame; includes GPU initialization and shutdown",
            "measurements": {name: {"median_ms": round(statistics.median(values), 3),
                                    "samples_ms": values, "range_ms": [min(values), max(values)]}
                             for name, values in times.items() if values},
            "client_binary": str(args.client_binary),
            "baseline_client_binary": str(args.baseline_client_binary) if args.baseline_client_binary else None,
            "game_dir": str(args.client_cwd),
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
