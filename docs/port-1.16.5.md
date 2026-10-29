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

- [ ] Read the target version metadata and registry information from the user's legally obtained 1.16.5 installation. This requires a separate pinned 1.16.5 preparation path; do not simply change `DATA_VERSION` or relabel the 26.3 bundle.
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
