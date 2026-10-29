#!/usr/bin/env python3
"""Real-game physics smoke test. Requires the owner's prepared Minecraft resources
and a working Horizon graphics backend. Never substitutes authored test resources.
Saves actual screenshots and client snapshot traces; screenshot review is still
needed to confirm visible pixels. All world edits happen in fresh scratch homes.
"""
import argparse
import json
import math
import subprocess
import tempfile
import time
from pathlib import Path

import lab


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, default=lab.ROOT / 'target/debug')
    parser.add_argument('--resources', type=Path, default=lab.RESOURCES)
    parser.add_argument('--out', type=Path, default=lab.ROOT / '.local/physics-visual-smoke')
    parser.add_argument('--in-frame', action='store_true', help='compare the fallback singleplayer path')
    parser.add_argument('--offscreen', action='store_true', help='use a supported offscreen graphics backend')
    args = parser.parse_args()
    if not (args.resources / 'horizon-resources.json').is_file():
        parser.exit(2, f'BLOCKED: no prepared Minecraft resources at {args.resources}; no game/screenshot test ran\n')
    bins = args.bin_dir.resolve()
    if not all((bins / name).is_file() for name in ['horizon-client', 'horizon-ctl']):
        parser.exit(2, 'BLOCKED: build horizon-client --bins with physics-unleashed first\n')
    args.out.mkdir(parents=True, exist_ok=True)
    out = Path(tempfile.mkdtemp(prefix='run-', dir=args.out)).resolve()
    result = {'status': 'failed', 'mode': 'in-frame' if args.in_frame else 'integrated', 'output': str(out)}
    with tempfile.TemporaryDirectory(prefix='physics-visual-') as scratch:
        scratch = Path(scratch)
        homes = {key: scratch / key.lower() for key in [
            'HORIZON_HOME', 'HORIZON_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME',
            'HORIZON_LAUNCHER_TRASH', 'HORIZON_LAUNCHER_CLIPBOARD', 'HORIZON_LAUNCHER_DESKTOP']}
        for path in homes.values():
            path.mkdir()
        env = lab.game_env(**homes, HORIZON_TELEMETRY_URL='off')
        socket = scratch / 'ctl.sock'

        def ctl(*words, timeout=30):
            command = [str(bins / 'horizon-ctl'), '--socket', str(socket), *map(str, words)]
            call = subprocess.run(command, cwd=lab.ROOT, env=env, capture_output=True, text=True, timeout=timeout)
            with (out / 'control.jsonl').open('a') as log:
                log.write(json.dumps({'command': list(words), 'code': call.returncode, 'stdout': call.stdout, 'stderr': call.stderr}) + '\n')
            if call.returncode:
                raise RuntimeError(f'{words}: {call.stderr or call.stdout}')
            reply = json.loads(call.stdout.strip().splitlines()[-1])
            if 'error' in reply:
                raise RuntimeError(reply)
            return reply

        def state():
            value = ctl('state')
            if 'physics' not in value:
                raise RuntimeError('game has no physics-unleashed client diagnostics')
            return value['physics']

        def wait_for(predicate, seconds=10):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                value = state()
                if predicate(value):
                    return value
                time.sleep(0.05)
            raise RuntimeError('timed out waiting for actual client physics state')

        def shot(name, height):
            eye = ctl('state')['player']['eye']
            dx, dy, dz = 8.5 - eye[0], height - eye[1], 8.5 - eye[2]
            ctl('look', math.degrees(math.atan2(-dx, dz)),
                math.degrees(math.atan2(-dy, math.hypot(dx, dz))))
            ctl('screenshot', str(out / f'{name}.png'))

        game = None
        game_log = (out / 'game.log').open('w')
        try:
            launch = [str(bins / 'horizon-client'), '--world', 'PhysicsVisual', '--creative', '--seed', '5', '--control-socket', str(socket), '--resources', str(args.resources.resolve())]
            if args.in_frame:
                launch.append('--in-frame')
            if args.offscreen:
                launch.append('--offscreen')
            # Own the process directly so a failed control connection or timeout
            # cannot leave an untracked GUI game running after scratch cleanup.
            game = subprocess.Popen(launch, cwd=lab.ROOT, env=env, stdin=subprocess.DEVNULL,
                                    stdout=game_log, stderr=subprocess.STDOUT)
            end = time.monotonic() + 120
            while not socket.exists():
                if game.poll() is not None:
                    raise RuntimeError(f'game exited {game.returncode}; see game.log')
                if time.monotonic() > end:
                    raise RuntimeError('game control endpoint did not start in 120 seconds')
                time.sleep(0.05)
            ctl('wait-ready', '120', timeout=130)
            # Deterministic stage in a NEW scratch world only.
            ctl('command', 'tp @s 12 77 12')
            ctl('wait-ready', '120', timeout=130)
            for command in ['fill 4 71 4 12 96 12 minecraft:air',
                            'fill 4 70 4 12 70 12 minecraft:stone', 'physics clear',
                            'physics spawn 8.5 80 8.5']:
                ctl('command', command)
            start = wait_for(lambda s: len(s['bodies']) == 1)
            identity = start['bodies'][0]['id']
            wait_for(lambda s: s['vertices'] > 0)
            shot('falling', 79)
            samples = []
            end = time.monotonic() + 9
            while time.monotonic() < end:
                sample = state()
                assert len(sample['bodies']) == 1 and sample['bodies'][0]['id'] == identity
                samples.append(sample['bodies'][0])
                time.sleep(0.05)
            ys = [b['position'][1] for b in samples]
            assert min(ys) > 71.2 and min(ys) < 72.0, ys
            assert any(b > a + 0.02 for a, b in zip(ys, ys[1:])), 'no real rebound observed'
            assert abs(ys[-1] - 71.5) < 0.1, 'body did not settle on the platform'
            shot('resting', 71)
            ctl('command', 'physics gravity 0 9.81 0')
            risen = wait_for(lambda s: s['bodies'] and s['bodies'][0]['position'][1] > 74)
            assert risen['bodies'][0]['id'] == identity
            shot('antigravity', risen['bodies'][0]['position'][1])
            ctl('command', 'physics clear')
            wait_for(lambda s: not s['bodies'] and s['vertices'] == 0)
            shot('cleared', 75)
            (out / 'poses.json').write_text(json.dumps(samples, indent=2))
            result.update(status='passed', body_id=identity, screenshots_need_visual_review=True)
        except Exception as error:
            result['error'] = str(error)
        finally:
            if game is not None:
                try:
                    if game.poll() is None and socket.exists():
                        ctl('quit', timeout=15)
                except Exception:
                    pass
                try:
                    game.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    game.terminate()
                    try:
                        game.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        game.kill()
                        game.wait(timeout=5)
            game_log.close()
            (out / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result))
    return 0 if result['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
