#!/usr/bin/env python3
"""Measure the client resource-loading startup path on first and cached runs.

Build a client binary beforehand; the timed runs exclude compilation.
By default the profiler exits after pack tables, before GPU initialization.
Pass --full-client for an offscreen, one-frame full startup smoke measurement.
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


def run(binary, resources, workdir, timeout, disable_cache=False, full_client=False):
    command = [
        str(binary), "--resources", str(resources), "--frames", "1",
        "--timeout-seconds", str(timeout),
    ]
    if full_client:
        command.append("--offscreen")
    log_path = workdir / "logs" / "latest.log"
    start = time.perf_counter()
    timed_out = False
    environment = dict(os.environ)
    environment.pop("HORIZON_DISABLE_PACK_RENDER_CACHE", None)
    environment.pop("HORIZON_PROFILE_PACK_LOAD", None)
    environment.update(RUST_LOG="info", HORIZON_HOME=str(workdir))
    if not full_client:
        environment["HORIZON_PROFILE_PACK_LOAD"] = "1"
    if disable_cache:
        environment["HORIZON_DISABLE_PACK_RENDER_CACHE"] = "1"
    try:
        result = subprocess.run(
            command, cwd=workdir, text=True, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, timeout=timeout + 10,
            env=environment,
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
    phases = re.search(r"pack phases mesh=([\d.]+)ms items=([\d.]+)ms extras=([\d.]+)ms", output)
    return {
        "exit_code": exit_code,
        "timed_out": timed_out,
        "elapsed_ms": round(elapsed, 2),
        "pack_interval_ms": None if pack is None or facts is None else round(pack - facts, 2),
        "pack_cumulative_ms": pack,
        "mesh_tables_ms": float((cached_tables or built_tables).group(1))
        if cached_tables or built_tables else None,
        "pack_phases_ms": dict(zip(("mesh", "items", "extras"),
                                   map(float, phases.groups())))
        if phases else None,
        "cache_hit": cached_tables is not None,
        "tail": output[-1500:] if timed_out or exit_code else None,
    }

def distribution(values):
    """Descriptive spread, since OS caches and concurrent work add variance."""
    if not values:
        return None
    median = statistics.median(values)
    return {
        "n": len(values),
        "median_ms": round(median, 2),
        "min_ms": round(min(values), 2),
        "max_ms": round(max(values), 2),
        "mad_ms": round(statistics.median(abs(v - median) for v in values), 2),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument("--workdir", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--full-client", action="store_true",
                        help="include GPU, window and game startup when supported")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    binary = args.binary.resolve()
    resources = args.resources.resolve()
    if not binary.is_file() or not (resources / "horizon-resources.json").is_file():
        parser.error("binary and prepared resources must exist")
    if args.workdir:
        args.workdir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
            prefix="horizon-startup-", dir=args.workdir) as scratch:
        workdir = Path(scratch)
        workdir.mkdir(parents=True, exist_ok=True)
        # Always write the cache once, then alternate the control and cached
        # conditions in reversed order every second pair. This limits bias
        # from OS warming, concurrent builds, and other run-order effects.
        first = run(binary, resources, workdir, args.timeout,
                    full_client=args.full_client)
        baseline, cached = [], []
        for i in range(args.repeats):
            conditions = [True, False] if i % 2 == 0 else [False, True]
            for disabled in conditions:
                result = run(binary, resources, workdir, args.timeout,
                             disable_cache=disabled, full_client=args.full_client)
                (baseline if disabled else cached).append(result)
        results = [first, *cached]
    baseline_intervals = [r["pack_interval_ms"] for r in baseline
                          if r["pack_interval_ms"] is not None]
    valid = [r["pack_interval_ms"] for r in results[1:]
             if r["pack_interval_ms"] is not None]
    summary = {
        "scope": "offscreen full-client startup" if args.full_client
        else "client startup through PackResources::load; excludes GPU/window/world startup",
        "binary": str(binary), "resources": str(resources),
        "uncached_baseline": baseline,
        "uncached_pack_median_ms": statistics.median(baseline_intervals)
        if baseline_intervals else None,
        "first_launch": results[0], "cached_launches": results[1:],
        "cached_pack_median_ms": statistics.median(valid) if valid else None,
        "spread": {
            metric: {
                "uncached": distribution([r[metric] for r in baseline if r[metric] is not None]),
                "cached": distribution([r[metric] for r in results[1:]
                                        if r[metric] is not None]),
            }
            for metric in ("elapsed_ms", "pack_interval_ms", "mesh_tables_ms")
        },
        "phases": {
            phase: {
                "uncached": distribution([r["pack_phases_ms"][phase] for r in baseline
                                          if r["pack_phases_ms"]]),
                "cached": distribution([r["pack_phases_ms"][phase] for r in results[1:]
                                        if r["pack_phases_ms"]]),
            }
            for phase in ("mesh", "items", "extras")
        },
    }
    print(json.dumps(summary, indent=2))
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(summary, indent=2) + "\n")
    if any(r["exit_code"] or r["timed_out"] for r in baseline + results):
        raise SystemExit("client startup incomplete; inspect the captured tail above")


if __name__ == "__main__":
    main()
