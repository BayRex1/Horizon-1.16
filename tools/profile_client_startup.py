#!/usr/bin/env python3
"""Measure the behaviour loader or block-and-item preparation at startup.

Build the behaviour_load_profile example in the checkout to be measured first:
    cargo build --release -p horizon-world --example behaviour_load_profile

Run the profiler from the repository root with the matching example binary.
For models, build horizon-client --example model_library_bench and pass
--kind models --binary PATH. Neither mode measures first rendered frame.
"""
import argparse
import json
import re
import statistics
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/behaviour_load_profile"))
    parser.add_argument("--resources", type=Path, default=Path(".local/horizon-client/26.3"))
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--kind", choices=("behaviours", "models"), default="behaviours")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be at least one")
    samples = []
    for _ in range(args.repeats):
        run = subprocess.run([str(args.binary.resolve()), str(args.resources.resolve())],
                             text=True, capture_output=True, check=True)
        if args.kind == "models":
            values = {key: float(value) for key, value in
                      re.findall(r"\b(block_ms|item_ms|total_ms)=([\d.]+)", run.stdout)}
        else:
            values = {key: float(value) for key, value in
                      re.findall(r"^(registry_ms|behaviours_[0-2]_ms)=([\d.]+)$", run.stderr, re.M)}
        if len(values) != (3 if args.kind == "models" else 4):
            raise RuntimeError(f"incomplete profile output: {run.stderr}")
        samples.append(values)
    summary = {key: {"median_ms": round(statistics.median(row[key] for row in samples), 3),
                     "samples_ms": [row[key] for row in samples]}
               for key in samples[0]}
    scope = ("block and item preparation; not full client startup" if args.kind == "models"
             else "behaviour loader only; not full client startup")
    result = {"scope": scope,
              "binary": str(args.binary), "resources": str(args.resources), "repeats": args.repeats,
              "metrics": summary}
    print(json.dumps(result, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
