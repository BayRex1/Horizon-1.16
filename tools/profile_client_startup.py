#!/usr/bin/env python3
"""Measure startup through the first rendered frame of a Horizon client build.

Uses the client's smoke report (elapsed_seconds measured from run startup).
Run the same command with baseline and candidate binaries on the same machine.
The run must render successfully; crashes and missing reports are rejected.

Example:
  python3 tools/profile_client_startup.py --binary target/release/horizon-client \
    --resources .local/horizon-client/26.3 --repeats 3 --vscreen
"""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--resources', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--vscreen', action='store_true')
    parser.add_argument('--offscreen', action='store_true',
                        help='Render to a texture instead of a presentation surface')
    parser.add_argument('--world', help='Optional world name for first playable frame measurement')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error('--repeats must be positive')
    binary = args.binary.resolve()
    resources = args.resources.resolve()
    results = []
    for repeat in range(args.repeats):
        with tempfile.TemporaryDirectory(prefix='horizon-startup-') as temp:
            report = Path(temp) / 'first-frame.json'
            cmd = [str(binary), '--resources', str(resources),
                   '--frames', '1', '--smoke-report', str(report),
                   '--timeout-seconds', '90']
            if args.offscreen:
                cmd.append('--offscreen')
            if args.world:
                cmd.extend(['--world', args.world, '--world-frames', '1'])
            if args.vscreen:
                cmd = ['xvfb-run', '-a', *cmd]
            proc = subprocess.run(cmd, stdout=subprocess.PIPE,
                                  stderr=subprocess.STDOUT, text=True,
                                  timeout=120)
            if proc.returncode != 0 or not report.is_file():
                raise RuntimeError(
                    f'run {repeat + 1} failed (exit={proc.returncode}):\n{proc.stdout[-4000:]}'
                )
            data = json.loads(report.read_text())
            if data.get('frames', 0) < 1 or data.get('error'):
                raise RuntimeError(f'run {repeat + 1} never rendered successfully: {data}')
            results.append(round(data['elapsed_seconds'] * 1000, 3))
    result = {
        'binary': str(binary), 'resources': str(resources),
        'first_frame_ms': results,
        'median_ms': round(statistics.median(results), 3),
        'mean_ms': round(statistics.mean(results), 3),
    }
    encoded = json.dumps(result, indent=2)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded + '\n')
    print(encoded)


if __name__ == '__main__':
    main()
