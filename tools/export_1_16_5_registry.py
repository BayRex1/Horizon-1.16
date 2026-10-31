#!/usr/bin/env python3
"""Export the Minecraft 1.16.5 block-state registry from vanilla's blocks.json report.

Input is the official DataGenerator report (run the 1.16.5 server jar with --reports).
Output matches horizon-world's facts.json schema. This exports registry identity/defaults;
it does not pretend to export collision, lighting, world-generation or block behaviour facts.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

VERSION = "1.16.5"
DATA_VERSION = 2586


def state_flags(name: str, properties: dict[str, str]) -> tuple[bool, bool]:
    # The report contains block-state properties, not evaluated FluidState/BlockState APIs.
    # Air identity is exact for vanilla 1.16.5. Water/lava and waterlogged states are
    # conservatively marked as fluid-bearing for Horizon's terrain heightmap inputs.
    air = name in {"minecraft:air", "minecraft:cave_air", "minecraft:void_air"}
    fluid = name in {"minecraft:water", "minecraft:lava"} or properties.get("waterlogged") == "true"
    return air, fluid


def export_report(report: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(report, dict) or not report:
        raise ValueError("blocks report must be a non-empty JSON object")

    indexed: list[tuple[int, str, dict[str, str], bool]] = []
    defaults: dict[str, int] = {}
    seen_ids: set[int] = set()

    for raw_name, block in report.items():
        name = raw_name if ":" in raw_name else "minecraft:" + raw_name
        if not isinstance(block, dict) or not isinstance(block.get("states"), list):
            raise ValueError(f"{name}: missing states array")
        states = block["states"]
        if not states:
            raise ValueError(f"{name}: empty states array")
        default_count = 0
        for state in states:
            if not isinstance(state, dict) or not isinstance(state.get("id"), int):
                raise ValueError(f"{name}: state without integer id")
            state_id = state["id"]
            if state_id < 0 or state_id in seen_ids:
                raise ValueError(f"{name}: invalid or duplicate global state id {state_id}")
            seen_ids.add(state_id)
            raw_props = state.get("properties", {})
            if not isinstance(raw_props, dict):
                raise ValueError(f"{name} state {state_id}: properties must be an object")
            properties = {str(k): str(v) for k, v in raw_props.items()}
            is_default = state.get("default") is True
            default_count += int(is_default)
            indexed.append((state_id, name, properties, is_default))
        if default_count != 1:
            raise ValueError(f"{name}: expected exactly one default state, got {default_count}")

    indexed.sort(key=lambda item: item[0])
    if [item[0] for item in indexed] != list(range(len(indexed))):
        raise ValueError("global block-state IDs must be contiguous and start at zero")

    rows = []
    for state_id, name, properties, is_default in indexed:
        air, fluid = state_flags(name, properties)
        state: Any = name if not properties else {"id": name, "properties": properties}
        rows.append([name, air, fluid, "", state])
        if is_default:
            defaults[name] = state_id

    if len(defaults) != len(report):
        raise ValueError("not every block has exactly one exported default state")
    return {
        "format": 1,
        "minecraft_version": VERSION,
        "data_version": DATA_VERSION,
        "states": rows,
        "defaults": dict(sorted(defaults.items())),
        "exporter": {
            "name": "horizon-export-1.16.5-registry",
            "source": "vanilla DataGenerator reports/blocks.json",
            "air_flag": "exact vanilla air block identity",
            "fluid_flag": "water, lava, or waterlogged property; conservative approximation",
            "limitations": [
                "fluid flag is not a full evaluated FluidState export",
                "collision and heightmap tag facts are not exported",
                "block behaviour, lighting and world generation are not exported",
            ],
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("blocks_report", type=Path, help="1.16.5 reports/blocks.json")
    parser.add_argument("--out", type=Path, required=True, help="output facts.json")
    args = parser.parse_args()
    try:
        report = json.loads(args.blocks_report.read_text(encoding="utf-8"))
        result = export_report(report)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n",
                            encoding="utf-8")
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        print(f"1.16.5 registry export failed: {exc}", file=sys.stderr)
        return 1
    print(f"Exported {len(result['states'])} block states for Minecraft {VERSION}")
    print(f"Exported {len(result['defaults'])} default block states")
    print("Note: registry identities are exported; this is not yet a complete Horizon resource bundle.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
