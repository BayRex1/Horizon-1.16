#!/usr/bin/env python3
"""Measure the client resource-loading startup path on first and cached runs.

Build a client binary beforehand; the timed runs exclude compilation.
The profiler exits just after loading the pack tables, before GPU initialization.
Use a disposable --workdir to keep pack caches and options isolated.
The resource directory must already be prepared.
"""
import argparse
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time


def run(binary, resources, workdir, timeout, disable_cache=False):
    command = [
        str(binary), "--resources", str(resources), "--frames", "1",
        "--timeout-seconds", str(timeout),
    ]
    log_path = workdir / "logs" / "latest.log"
    start = time.perf_counter()
    timed_out = False
    try:
        result = subprocess.run(
            command, cwd=workdir, text=True, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, timeout=timeout + 10,
            env={**os.environ, "RUST_LOG": "info", "HORIZON_HOME": str(workdir),
                 "HORIZON_PROFILE_PACK_LOAD": "1",
                 **({"HORIZON_DISABLE_PACK_RENDER_CACHE": "1"} if disable_cache else {})},
        )
        output = result.stdout
        exit_code = result.returncode
    except subprocess.TimeoutExpired as error:
        timed_out = True
        output = (error.stdout or b"").decode("utf-8", "replace")
        exit_code = None
    elapsed = (time.perf_counter() - start) * 1000
    if log_path.exists():
        output += "\n" + log_path.read_text(errors="replace")
    # These logs are cumulative since game startup. Pull out the pack table
    # interval, including its upstream asset/facts loading separately.
    def milestone(name):
        match = re.search(rf"{re.escape(name)}: ([\d.]+)s", output)
        return None if match is None else float(match.group(1)) * 1000
    facts = milestone("load biomes+env")
    pack = milestone("load pack tables")
    cached_tables = re.search(r"cached pack mesh tables in ([\d.]+)ms", output)
    built_tables = re.search(r"built pack mesh tables in ([\d.]+)ms", output)
    return {
        "exit_code": exit_code,
        "timed_out": timed_out,
        "elapsed_ms": round(elapsed, 2),
        "pack_interval_ms": None if pack is None or facts is None else round(pack - facts, 2),
        "pack_cumulative_ms": pack,
        "mesh_tables_ms": float((cached_tables or built_tables).group(1))
        if cached_tables or built_tables else None,
        "cache_hit": cached_tables is not None,
        "tail": output[-1500:] if timed_out or exit_code else None,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument("--workdir", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    binary = args.binary.resolve()
    resources = args.resources.resolve()
    if not binary.is_file() or not (resources / "horizon-resources.json").is_file():
        parser.error("binary and prepared resources must exist")
    with tempfile.TemporaryDirectory(prefix="horizon-startup-") as scratch:
        workdir = args.workdir.resolve() if args.workdir else Path(scratch)
        workdir.mkdir(parents=True, exist_ok=True)
        baseline = [run(binary, resources, workdir, args.timeout, disable_cache=True)
                    for _ in range(args.repeats)]
        results = [run(binary, resources, workdir, args.timeout)
                   for _ in range(args.repeats + 1)]
    baseline_intervals = [r["pack_interval_ms"] for r in baseline
                          if r["pack_interval_ms"] is not None]
    valid = [r["pack_interval_ms"] for r in results[1:]
             if r["pack_interval_ms"] is not None]
    summary = {
        "scope": "client startup through PackResources::load; excludes GPU/window/world startup",
        "binary": str(binary), "resources": str(resources),
        "uncached_baseline": baseline,
        "uncached_pack_median_ms": statistics.median(baseline_intervals)
        if baseline_intervals else None,
        "first_launch": results[0], "cached_launches": results[1:],
        "cached_pack_median_ms": statistics.median(valid) if valid else None,
    }
    print(json.dumps(summary, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(summary, indent=2) + "\n")
    if any(r["exit_code"] or r["timed_out"] for r in baseline + results):
        raise SystemExit("client startup incomplete; inspect the captured tail above")


if __name__ == "__main__":
    main()
