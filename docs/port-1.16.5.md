# Minecraft Java 1.16.5 port

## Target and baseline

Target: **Minecraft: Java Edition 1.16.5** (protocol **754**, data version **2586**, Java 8).

This branch starts from Horizon's Rust implementation for Minecraft 26.3 (protocol 777, data version 5023, Java 25). It is not a 1.16.5 game merely because the protocol helpers compile: block registries, world-generation data, assets, save format, and gameplay rules also differ.

## Priority decision: singleplayer first

The first playable milestone is now a **local singleplayer world**. Do not spend the next implementation steps wiring remote-server connections. The existing client already has a local-world loading path and an integrated game/server tick path, but those paths currently use modern-version assumptions. The job is to backport the local world path to 1.16.5, not to invent a new server connection as a shortcut.

Implementation order:
1. Load the user's own 1.16.5 resources and registry data without committing Mojang jars/assets.
2. Create a new local world from the existing Create World flow; seed and game mode must reach the local world request.
3. Generate the first terrain chunks locally and put them into the resident chunk/streaming pipeline.
4. Render the generated chunks, find a valid spawn, and make movement/collision work.
5. Save and reopen the world using the 1.16.5-compatible data version and region/chunk layout.
6. Optimize chunk generation, meshing, draw distance, and frame pacing only after the first local world is visible.
7. **Defer multiplayer/server compatibility** until the singleplayer acceptance checks pass.

## Milestone 1 — build and identify the local-world seams

- [x] Fix the Rust compile error in the legacy biome helper (return `Ok(cells)`).
- [x] Keep CI green on the current branch before adding more changes.
- [x] Trace Create World -> WorldRequest -> PreparedWorld -> integrated game tick -> streaming -> renderer.
- [x] Mark the first data/version dependency: `Assets::load` rejects resource bundles whose `data_version` differs from the single compiled `horizon_world::DATA_VERSION`; the preparer is pinned by `research-lock.json` to 26.3 / Java 25. The current Create World UI and integrated server are present, but they do not make the game 1.16.5-compatible.
- [ ] Establish a local-world test that does not require a network connection.

## Milestone 2 — 1.16.5 local world bootstrap

- [x] Add `tools/prepare_1_16_5.py`: it pins official 1.16.5 manifest/client/server SHA-1 values, validates the player's local files, and stages client `assets/minecraft` plus server `data/minecraft` inputs. It never downloads or commits the game jars/assets.
- [ ] Turn the staged inputs into a complete Horizon bundle. The staging tool intentionally does not emit `horizon-resources.json` because the version-specific registry/state exporter and worldgen adapters are not implemented yet.
- [ ] Build a version-specific registry/resource bundle for block states, biomes, dimensions, features, and tags. Keep the existing 26.3 resource format working until the 1.16.5 bundle is parsed and validated end-to-end.
- [ ] Make the Create World flow construct a world with 1.16.5 defaults: overworld dimension height, seed, game mode, difficulty, structures and bonus-chest options.
- [ ] Ensure the first local chunks are generated in-process; no TCP connection or remote chunk packet is involved.
- [ ] Add deterministic tests for a fixed seed and key terrain/spawn invariants.

## Milestone 3 — terrain visible and playable

- [ ] Generate and install the spawn-area chunk columns into the resident world.
- [ ] Use the 1.16.5 vertical range (Y=0..255; the modern -64..319 range is incorrect) throughout chunk storage, heightmaps, lighting and collision.
- [ ] Calculate lighting and build meshes for local chunks.
- [ ] Place the player at a safe spawn and verify walking, jumping, collision, block targeting, breaking and placing.
- [ ] Test create -> play -> save -> quit -> reopen with the same seed and terrain.

## Milestone 4 — assets, persistence and performance

- [ ] Load 1.16.5 textures, models, sounds and language files from the user's installation; do not ship game assets in the repository.
- [ ] Read/write 1.16.5 save metadata and chunk NBT correctly; do not label modern save data as 1.16.5.
- [ ] Add a low-end preset after rendering is correct: short render distance, fewer effects, bounded chunk build queues, and sensible memory use.
- [ ] Measure frame time, chunk generation time and memory. Do not claim FPS gains without measurements on actual hardware.
- [ ] Run formatting, unit tests and CI after each coherent change.

## Deferred milestone — multiplayer

Protocol-754 groundwork already exists in `crates/horizon-protocol/src/legacy_1_16_5.rs`, and a chunk event bridge exists in the client. It is **not the current priority**. Once singleplayer works, revisit packet login, online-mode authentication, server chunk decoding, movement/interactions, and clean disconnect handling. Do not claim server compatibility until an end-to-end test succeeds.

## Technical risks

1. The port is a substantial backport from 26.3, not a version-string change.
2. Existing generation and gameplay code depends on modern registry/resource data and world-height assumptions.
3. The renderer's graphics API/driver requirements are independent of the protocol and may limit old Intel GPUs.
4. Rust/toolchain/dependency requirements may exceed what the target laptop can run comfortably.

## Acceptance criteria

**Singleplayer milestone:** launch the client, create a world with a fixed seed without network access, see locally generated terrain, spawn safely, move and interact with blocks, save, exit, and reopen the same world without a crash. A successful Rust compilation alone is not sufficient.


## Version-specific preparation — first implementation

The 1.16.5 input stager is a developer tool, separate from the existing 26.3 preparer, so the current version's research lock and generated constants stay untouched. It checks official hashes before extracting anything, rejects unsafe archive paths, and writes a report that explicitly marks the output as staged inputs rather than a playable bundle. A local server jar is optional: if omitted, the script downloads the pinned official 1.16.5 server jar into a temporary directory and removes it afterward.

Example (Windows PowerShell; use the path to your Minecraft launcher install):

```powershell
py -3 tools/prepare_1_16_5.py `
  --minecraft-dir "$env:APPDATA\.minecraft" `
  --out ".local\horizon-client\1.16.5-inputs"
```

You do not need Linux or a running Minecraft server. This script is for developers preparing source data; someone testing a prebuilt Horizon Windows package should not need to run it at all.

The output contains `client-assets/`, `server-data/`, and `horizon-1.16.5-inputs.json`. It is an intermediate dataset only. The remaining critical work is a 1.16.5 Java-side exporter compatible with that release's obfuscated classes, followed by Rust adapters for its block-state IDs, biome registry, noise settings/density functions, carvers and placed features. Until those exist, do not point the current client at the staged folder or describe it as a working resource bundle.


## Public research sources — no manual JAR downloads for the tester

The port can be researched and implemented using public technical references and versioned data repositories. These are development inputs; the tester on Windows 7 should receive a prebuilt client and should not run this preparation script.

- **Versioned generated game data:** [misode/mcmeta](https://github.com/misode/mcmeta). Its repository describes versioned snapshots of generated data and assets, with a `summary` branch containing condensed block-state data and registries. Select the exact 1.16.5 revision/branch before importing anything; do not use the repository's current-version data for this port.
- **Block, item, biome and protocol reference data:** [PrismarineJS/minecraft-data](https://github.com/PrismarineJS/minecraft-data). Its supported-version list includes Java 1.16.5 and it provides machine-readable data useful for building Rust lookup tables and protocol fixtures. Treat it as reference data, not as the terrain generator itself.
- **Chunk storage and network representation:** [Minecraft Wiki — Java Edition protocol/Chunk format](https://minecraft.wiki/w/Java_Edition_protocol/Chunk_format). Use the version-specific sections to implement palettes, block-state indices, biome data and chunk-section serialization.
- **World-generation concepts and data-pack schema:** [Minecraft Wiki — Custom world generation](https://minecraft.wiki/w/Custom_world_generation), [Noise settings](https://minecraft.wiki/w/Noise_settings), and [Biome definition (Java Edition)](https://minecraft.wiki/w/Biome_definition_(Java_Edition)). These explain the data model, but the exact 1.16.5 defaults and algorithm behavior must be verified against versioned data.
- **Historical 1.16.5 worldgen examples:** [KubeJS 1.16.5 worldgen reference](https://kubejs.com/wiki/tutorials/worldgen/1605) and [slicedlime/examples](https://github.com/slicedlime/examples) can help clarify feature ordering and legacy worldgen conventions.

### How to use these sources

1. Import only data verified to match Java 1.16.5 (data version 2586); record source URL, version and checksum for each generated table.
2. Generate Rust tables for block identifiers/states, biome identifiers/properties, tags and collision shapes from machine-readable sources where possible.
3. Implement the 1.16.5 terrain algorithm in Rust: noise settings and density calculations, biome selection, caves/carvers, surface rules, placed features and structures. Public documentation alone does not guarantee bit-for-bit seed parity.
4. Add deterministic fixture tests for known seeds and compare biome/terrain samples against independently generated 1.16.5 reference outputs.
5. Keep the JAR staging script optional as a verification/fallback tool for developers. It is not a runtime dependency and is not something the Windows 7 tester should have to use.

**Important:** public reference data can remove the need for a developer to manually download the JARs for every iteration, but it does not make a complete generator appear automatically. The Rust implementation and seed-based validation remain required. Check each source's license/terms before copying or redistributing its data.
