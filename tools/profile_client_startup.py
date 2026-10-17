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
    parser.add_argument("--compare-binary", type=Path,
                        help="baseline executable; --binary is the candidate, run alternately")
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--parity", action="store_true", help="compare model fingerprints")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be at least one")
    if args.warmups < 0:
        parser.error("--warmups must be nonnegative")
    if args.parity and args.kind != "models":
        parser.error("--parity requires --kind models")

    def measure(binary):
        argv = [str(binary.resolve()), str(args.resources.resolve())]
        if args.parity:
            argv.append("--parity")
        run = subprocess.run(argv, text=True, capture_output=True, check=True)
        if args.kind == "models":
            values = {key: float(value) for key, value in
                      re.findall(r"\b(block_ms|item_ms|total_ms)=([\d.]+)", run.stdout)}
        else:
            values = {key: float(value) for key, value in
                      re.findall(r"^(registry_ms|behaviours_[0-2]_ms)=([\d.]+)$", run.stderr, re.M)}
        if len(values) != (3 if args.kind == "models" else 4):
            raise RuntimeError(f"incomplete profile output: {run.stdout} {run.stderr}")
        fingerprint = re.search(r"parity_models=\d+ fingerprint=([0-9a-f]+)", run.stdout)
        if args.parity and not fingerprint:
            raise RuntimeError(f"missing parity fingerprint: {run.stdout}")
        return values, (fingerprint.group(1) if fingerprint else None)

    binaries = {"candidate": args.binary}
    if args.compare_binary:
        binaries["baseline"] = args.compare_binary
    samples = {name: [] for name in binaries}
    fingerprints = {name: set() for name in binaries}
    for iteration in range(args.warmups + args.repeats):
        order = list(binaries)
        if iteration % 2:
            order.reverse()
        for name in order:
            values, fingerprint = measure(binaries[name])
            if iteration >= args.warmups:
                samples[name].append(values)
                if fingerprint:
                    fingerprints[name].add(fingerprint)
    summary = {
        name: {key: {
            "median_ms": round(statistics.median(row[key] for row in entries), 3),
            "min_ms": round(min(row[key] for row in entries), 3),
            "max_ms": round(max(row[key] for row in entries), 3),
            "samples_ms": [row[key] for row in entries]}
            for key in entries[0]}
        for name, entries in samples.items()
    }
    scope = ("block and item preparation; not full client startup" if args.kind == "models"
             else "behaviour loader only; not full client startup")
    result = {"scope": scope, "binaries": {k: str(v) for k, v in binaries.items()},
              "resources": str(args.resources), "repeats": args.repeats,
              "warmups": args.warmups, "alternating_order": True, "metrics": summary}
    if args.compare_binary:
        result["median_candidate_minus_baseline_ms"] = {
            key: round(summary["candidate"][key]["median_ms"] - summary["baseline"][key]["median_ms"], 3)
            for key in summary["baseline"]}
    if args.parity:
        result["fingerprints"] = {name: sorted(values) for name, values in fingerprints.items()}
        if len({value for values in fingerprints.values() for value in values}) != 1:
            raise RuntimeError(f"render parity fingerprints differ: {result['fingerprints']}")
    print(json.dumps(result, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
