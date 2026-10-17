#!/usr/bin/env python3
"""Check every block fact and shape-sharing partition across prepared/JSON loading.

Requires locally exported game resources and a built block_facts_bench example.
Example:
    cargo build --profile fast -p horizon-block-facts --example prepared_block_facts_bench
    python3 tools/test_block_facts_prepared.py
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FILES = ("facts.json", "gameplay-facts.json", "block-behaviours.json")


def run(exe, root, mode):
    return subprocess.check_output([str(exe), str(root), mode], text=True)


def digest(exe, root):
    output = run(exe, root, "digest")
    match = re.search(r"states=(\d+) shared_shapes=(\d+) digest=([0-9a-f]+)", output)
    if not match:
        raise AssertionError(f"Missing digest in {output!r}")
    return match.groups()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--resources", type=Path, default=ROOT / ".local/horizon-client/26.3")
    parser.add_argument(
        "--exe", type=Path, default=ROOT / "target/fast/examples/prepared_block_facts_bench"
    )
    args = parser.parse_args()
    exe = args.exe.resolve()
    with tempfile.TemporaryDirectory(prefix="horizon-facts-") as temp:
        root = Path(temp)
        for name in FILES:
            shutil.copy2(args.resources / name, root / name)
        baseline = digest(exe, root)
        run(exe, root, "prepare")
        cache = root / "gameplay-facts-prepared.bin"
        assert cache.is_file()
        assert digest(exe, root) == baseline, "Prepared facts changed game behavior"
        cached = cache.read_bytes()
        run(exe, root, "prepare")
        assert cache.read_bytes() == cached, "Unchanged source rebuilt the prepared cache"
        # The same three facts files are linked through a resource-pack root;
        # visual-only overrides must preserve the block-state facts.
        pack_root = root / "pack-overlay"
        pack_root.mkdir()
        for name in FILES:
            (pack_root / name).symlink_to(root / name)
        (pack_root / cache.name).symlink_to(cache)
        (pack_root / "assets/test").mkdir(parents=True)
        (pack_root / "assets/test/custom.txt").write_text("resource-pack-only override")
        assert digest(exe, pack_root) == baseline, "Visual-only pack overlay changed block facts"
        cache.write_bytes(b"invalid prepared file")
        assert digest(exe, root) == baseline, "Corrupt cache did not fall back to JSON"
        cache.write_bytes(cached)
        cache.write_bytes(cached[:-1] + bytes([cached[-1] ^ 0x20]))
        assert digest(exe, root) == baseline, "Tampered payload bypassed checksum"
        cache.write_bytes(cached)
        cache.write_bytes(cached[:4] + bytes([cached[4] ^ 0x40]) + cached[5:])
        assert digest(exe, root) == baseline, "Tampered checksum bypassed verification"
        cache.write_bytes(cached)
        cache.write_bytes(b"HBF0" + cached[4:])
        assert digest(exe, root) == baseline, "Incompatible cache version bypassed validation"
        cache.write_bytes(cached)

        # Preserve original source size and modification time while changing
        # an individual numeric fact: metadata-only validation would miss it.
        gameplay = root / "gameplay-facts.json"
        previous = gameplay.stat()
        original = gameplay.read_bytes()
        match = re.search(rb'"emission"\s*:\s*([0-9])', original)
        assert match, "No single-digit emission in exported facts"
        replacement = bytes([(match.group(1)[0] - 48 + 1) % 10 + 48])
        edited = original[:match.start(1)] + replacement + original[match.end(1):]
        assert len(edited) == len(original)
        gameplay.write_bytes(edited)
        os.utime(gameplay, ns=(previous.st_atime_ns, previous.st_mtime_ns))
        assert gameplay.stat().st_size == previous.st_size
        assert gameplay.stat().st_mtime_ns == previous.st_mtime_ns
        updated_same_stamp = digest(exe, root)
        assert updated_same_stamp != baseline, "Source changed with preserved metadata used stale cache"
        run(exe, root, "prepare")
        assert digest(exe, root) == updated_same_stamp
        gameplay.write_bytes(original)
        run(exe, root, "prepare")
        assert digest(exe, root) == baseline

        # Changing gameplay facts must invalidate the prepared table, even if
        # the active registry remains identical.
        gameplay = root / "gameplay-facts.json"
        source = json.loads(gameplay.read_text())
        emission = source["states"][0].get("emission", 0)
        source["states"][0]["emission"] = 7 if emission != 7 else 6
        gameplay.write_text(json.dumps(source))
        updated = digest(exe, root)
        assert updated != baseline, "Stale cache survived modified source JSON"
        run(exe, root, "prepare")
        assert digest(exe, root) == updated, "Rebuilt prepared facts differ from JSON"
        print(f"PASS: {baseline[0]} states, {baseline[1]} shared shapes, exact digest parity")
        print(f"PASS: corrupt cache falls back to JSON ({len(cached):,} cache bytes)")
        print("PASS: incompatible version, corrupt payload, and preserved-metadata source changes")
        print("PASS: resource-pack overlay reuses identical state facts")
        print("PASS: changed gameplay facts invalidate and rebuild the cache")


if __name__ == "__main__":
    main()
