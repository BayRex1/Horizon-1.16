#!/usr/bin/env python3
"""Profile Behaviours::load or Assets::load and item preparation.

For --metric assets/startup, build `cargo build --profile fast -p horizon-client
--example model_library_bench` and supply its binary with --binary. These
metrics do not measure rendering the first frame or full game startup.
"""
import argparse
import json
import re
import statistics
import subprocess
from pathlib import Path


def cache_path(resources):
    if resources.name == "mod-content" and (resources / "horizon-mod-content.json").is_file():
        return resources.parent / "mod-content-block-registry-cache.bin"
    return resources / "block-registry-cache.bin"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/behaviour_load_profile"))
    parser.add_argument("--resources", type=Path, default=Path(".local/horizon-client/26.3"))
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--metric", choices=["behaviours", "assets", "startup"], default="behaviours",
                        help="Profile behaviours, Assets::load, or Assets::load plus item preparation")
    parser.add_argument("--cold-cache", action="store_true",
                        help="Delete the derived registry cache before each sample (use a disposable root)")
    parser.add_argument("--warm-cache", action="store_true",
                        help="Prime the derived cache with a run excluded from measured samples")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be at least one")
    if (args.cold_cache or args.warm_cache) and args.metric == "behaviours":
        parser.error("cache modes require --metric assets or startup")
    if args.cold_cache and args.warm_cache:
        parser.error("select at most one cache mode")
    command = [str(args.binary.resolve()), str(args.resources.resolve())]
    if args.warm_cache:
        subprocess.run(command, text=True, capture_output=True, check=True)
    samples = []
    for _ in range(args.repeats):
        if args.cold_cache:
            cache_path(args.resources).unlink(missing_ok=True)
        run = subprocess.run(command, text=True, capture_output=True, check=True)
        if args.metric in ("assets", "startup"):
            keys = "block_ms" if args.metric == "assets" else "block_ms|item_ms|total_ms"
            values = {key: float(value) for key, value in
                      re.findall(rf"\b({keys})=([\d.]+)", run.stdout)}
        else:
            values = {key: float(value) for key, value in
                      re.findall(r"^(registry_ms|behaviours_[0-2]_ms)=([\d.]+)$", run.stderr, re.M)}
        expected = 4 if args.metric == "behaviours" else (1 if args.metric == "assets" else 3)
        if len(values) != expected:
            raise RuntimeError(f"incomplete profile output: {run.stderr}")
        samples.append(values)
    summary = {key: {"median_ms": round(statistics.median(row[key] for row in samples), 3),
                     "min_ms": min(row[key] for row in samples),
                     "max_ms": max(row[key] for row in samples),
                     "mad_ms": round(statistics.median(abs(row[key] - statistics.median(sample[key] for sample in samples)) for row in samples), 3),
                     "samples_ms": [row[key] for row in samples]}
               for key in samples[0]}
    scope = {"behaviours": "behaviour loader only; not full client startup",
             "assets": "Assets::load including models and sprites; not full client startup",
             "startup": "Assets::load plus item renderer preparation; not time to first frame"}
    result = {"cache": "cold" if args.cold_cache else ("warm" if args.warm_cache else "as-present"),
              "scope": scope[args.metric],
              "binary": str(args.binary), "resources": str(args.resources), "repeats": args.repeats,
              "metrics": summary}
    print(json.dumps(result, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
