#!/usr/bin/env python3
"""Compare Frost Walker's replace_disk with vanilla: the cells, their order, the
predicate (water source, air above, unobstructed) and the frosted ice set.

Java runs the frost_walker enchantment's own replace_disk (ReplaceDisk.apply, with
its codec-loaded predicate) on synthetic levels; Rust runs horizon_client::frost on
the same levels. Levels cover radius 3 to 16, the water, flowing-water, air and
solid mixes around the feet, and blocking boxes that touch, cross and miss the disk.
"""
import argparse
import os
from pathlib import Path
import platform
import random
import struct
import subprocess
import json
try:
    from . import lab, test_rust_parity as base
except ImportError:
    import lab
    import test_rust_parity as base


LEVELS = [1, 2, 3, 14]
DEFAULTS = ['air', 'water', 'stone']
BELOW = {'water': 0.55, 'flowing': 0.15, 'stone': 0.10, 'air': 0.10, 'ice': 0.10}
ABOVE = {'air': 0.45, 'cave_air': 0.10, 'void_air': 0.05, 'water': 0.20, 'stone': 0.20}


def bits(value):
    # Java and Rust read the IEEE bit pattern of each double from hex.
    return struct.pack('>d', float(value)).hex()


def pick(rng, weights):
    total = sum(weights.values())
    roll = rng.random() * total
    for kind, weight in weights.items():
        roll -= weight
        if roll < 0:
            return kind
    return next(iter(weights))


def line(level, position, default, cells, boxes):
    cell_text = ';'.join(f'{x},{y},{z},{kind}' for (x, y, z), kind in cells.items()) or '-'
    box_text = ';'.join(','.join(bits(v) for v in box) for box in boxes) or '-'
    return f"{level} {','.join(bits(v) for v in position)} {default} {cell_text} {box_text}"


def random_case(rng, level, position, reach):
    feet = [int(position[0] // 1), int(position[1] // 1), int(position[2] // 1)]
    default = rng.choice(DEFAULTS)
    cells = {}
    for x in range(feet[0] - reach, feet[0] + reach + 1):
        for z in range(feet[2] - reach, feet[2] + reach + 1):
            if rng.random() < 0.8:
                cells[(x, feet[1] - 1, z)] = pick(rng, BELOW)
            if rng.random() < 0.5:
                cells[(x, feet[1], z)] = pick(rng, ABOVE)
    boxes = []
    for _ in range(rng.randrange(4)):
        kind = rng.randrange(5)
        px, py, pz = position
        if kind == 0:
            # The player's own box: it touches the disk's top face and must not block.
            boxes.append([px - 0.3, feet[1], pz - 0.3, px + 0.3, feet[1] + 1.8, pz + 0.3])
        elif kind == 1:
            # A box that crosses the water layer under the feet.
            x, z = feet[0] + rng.randint(-reach, reach), feet[2] + rng.randint(-reach, reach)
            boxes.append([x + 0.25, feet[1] - 0.5, z + 0.25, x + 0.75, feet[1] + 0.5, z + 0.75])
        elif kind == 2:
            # Exactly one block's box: intersects that cell with volume.
            x, z = feet[0] + rng.randint(-reach, reach), feet[2] + rng.randint(-reach, reach)
            boxes.append([x, feet[1] - 1, z, x + 1, feet[1], z + 1])
        elif kind == 3:
            # A box that only touches a face of a water cell.
            x, z = feet[0] + rng.randint(-reach, reach), feet[2] + rng.randint(-reach, reach)
            boxes.append([x + 1, feet[1] - 1, z, x + 2, feet[1], z + 1])
        else:
            x0, z0 = rng.uniform(-reach, reach) + feet[0], rng.uniform(-reach, reach) + feet[2]
            boxes.append([x0, feet[1] - 1 + rng.random(), z0, x0 + rng.random() * 2, feet[1] + rng.random() * 2, z0 + rng.random() * 2])
    return line(level, position, default, cells, boxes)


def corpus():
    rng = random.Random(0x46524F5354)
    positions = [
        [0.5, 64.0, 0.5], [0.5, 65.0, 0.5], [0.3, 64.0, 0.7], [0.0, 64.0, 0.0],
        [-3.5, -64.0, 12.25], [1000.9, 70.0, -2000.1], [-0.0, 0.0, -0.0],
        [2.999999, 80.0, -7.000001], [0.1 + 0.2, 63.99999, 4.5], [-17.5, 319.0, 1e6 % 1000],
    ]
    # Full water disks, flat and lined with air, at every radius: order and count.
    for level in LEVELS:
        for position in positions:
            reach = 17
            feet = [int(position[0] // 1), int(position[1] // 1), int(position[2] // 1)]
            cells = {(x, feet[1] - 1, z): 'water' for x in range(feet[0] - reach, feet[0] + reach + 1)
                     for z in range(feet[2] - reach, feet[2] + reach + 1)}
            yield line(level, position, 'air', cells, [])
            yield line(level, position, 'water', {}, [])
            yield line(level, position, 'water', {(feet[0], feet[1] + 1, feet[2]): 'air'}, [])
            yield line(level, position, 'air', {(feet[0], feet[1] + 1, feet[2]): 'water'}, [])
    # The strict distance: a cell whose centre is exactly the radius away is not in the disk.
    for level in LEVELS:
        yield line(level, [0.5, 64.0, 0.5], 'air', {(x, 63, z): 'water' for x in range(-4, 5) for z in range(-4, 5)}, [])
    # Random worlds: the predicate's every combination around the feet.
    for level in LEVELS:
        for _ in range(900):
            position = [rng.choice([rng.uniform(-40, 40), rng.randint(-40, 40) + 0.5]),
                        rng.choice([64.0, 63.0, 65.0, rng.uniform(-64, 320)]),
                        rng.choice([rng.uniform(-40, 40), rng.randint(-40, 40) + 0.5])]
            yield random_case(rng, level, position, reach=rng.choice([3, 4, 6, 17]))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prepare', action='store_true')
    args = parser.parse_args()
    build = lab.LOCAL / 'agents/frost-walker-parity'
    build.mkdir(parents=True, exist_ok=True)
    if args.prepare:
        lab.prepare()
    lock, jars, libraries = base.verify_artifacts()
    oracle = lab.ROOT / 'mods/horizon/src/test/java/dev/horizonlab/parity/VanillaFrostWalkerOracle.java'
    sources = [oracle, Path(__file__), lab.ROOT / 'tools/test_rust_parity.py', lab.ROOT / 'tools/lab.py',
               lab.ROOT / 'Cargo.toml', lab.ROOT / 'Cargo.lock', lab.ROOT / 'crates/horizon-client/src/frost.rs',
               lab.ROOT / 'crates/horizon-client/examples/probe/frost_walker_fixture.rs',
               lab.ROOT / 'crates/horizon-core/Cargo.toml', lab.ROOT / 'crates/horizon-world/Cargo.toml',
               lab.ROOT / 'crates/horizon-client/Cargo.toml',
               *sorted((lab.ROOT / 'crates/horizon-core/src').rglob('*.rs')),
               *sorted((lab.ROOT / 'crates/horizon-world/src').rglob('*.rs'))]
    tested = base.source_hashes(sources)
    fixtures = list(corpus())
    inputs = build / 'inputs.txt'
    inputs.write_text(''.join(value + '\n' for value in fixtures), encoding='utf-8')
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    deps = os.pathsep.join(map(str, libraries))

    def run(command, name):
        with (build / (name + '.log')).open('w', encoding='utf-8') as log:
            base.run(list(map(str, command)), stdout=log, stderr=subprocess.STDOUT)

    run(['javac', '--release', lock['minecraft']['java_major'], '-proc:none', '-sourcepath',
         str(lab.ROOT / 'mods/horizon/src/test/java'), '-cp', str(jars['server']) + os.pathsep + deps,
         '-d', classes, oracle], 'compile-java')
    run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-client', '--example', 'client_probe'], 'compile-rust')
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast/examples' / ('client_probe.exe' if os.name == 'nt' else 'client_probe')
    run([binary, 'frost_walker_fixture', inputs, build / 'rust.txt'], 'rust')
    run(['java', '-Xmx1G', '-cp', os.pathsep.join([str(classes), str(jars['server']), deps]),
         'dev.horizonlab.parity.VanillaFrostWalkerOracle', inputs, build / 'server.txt'], 'server')
    matched = base.compare(build / 'server.txt', build / 'rust.txt')
    print(f'server: {matched:,} frost walker output lines match Rust', flush=True)
    base.ensure_sources_unchanged(sources, tested)
    report = {'schema': 1, 'minecraft': lock['minecraft']['version'], 'platform': platform.platform(),
              'fixtures': len(fixtures), 'sources_sha256': tested,
              'references': [{'side': 'server', 'jar_sha256': lab.digest(jars['server'], 'sha256'),
                              'matched_output_lines': matched}],
              'scope': 'Frost Walker replace_disk: the enchantment registry entry and its codec-loaded predicate and '
                       'radius (ReplaceDisk.apply, levels 1 to 14), vanilla BlockPos.betweenClosed order, the distance '
                       'test, water source and air-above predicates, UnobstructedPredicate against blocking boxes, and '
                       'the block_place trigger per set cell. Worlds are synthetic; the level answers getBlockState, '
                       'setBlockAndUpdate and getEntities. Not covered: when the effect runs (location and ground '
                       'checks in LivingEntity and the enchantment tick), the melt tick of frosted ice, and the '
                       'packet and rendering of the changed blocks.'}
    lab.write_json(build / 'report.json', report)
    print(f'Frost walker parity passed: {build / "report.json"}')


if __name__ == '__main__':
    main()
