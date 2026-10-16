#!/usr/bin/env python3
"""Profile the isolated block-entity loader, not end-to-end menu startup.

Build first: cargo build --release -p horizon-client --example block_entity_load_profile
Run from the repo root with prepared resources available under --resources.
"""
import argparse
import json
import re
import statistics
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/block_entity_load_profile"))
    parser.add_argument("--resources", type=Path, default=Path(".local/horizon-client/26.3"))
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be at least one")
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
                          for key in samples[0]}}
    print(json.dumps(result, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
