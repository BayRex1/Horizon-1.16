# Minecraft Java 1.16.5 port

## Target and current baseline

Target: **Minecraft: Java Edition 1.16.5** (protocol **754**, data version **2586**, Java **8**).

This branch starts from Horizon's existing Rust implementation, which is currently pinned to Minecraft 26.3 (protocol 777, data version 5023, Java 25). That implementation is not wire-compatible with 1.16.5. Do not change only `VERSION_ID` or `PROTOCOL_VERSION` and call the result a port: the packet IDs and payloads, login flow, registries, chunk encoding, item/block data, and resource formats must match the target release.

## Porting strategy

Keep this as an isolated branch until the old protocol and gameplay path have tests. Preserve upstream history and the MIT license/notices.

### Milestone 1 — establish a reproducible baseline

- [ ] Record the exact upstream commit and make sure the existing branch builds on a supported Rust toolchain.
- [ ] Run existing Rust tests and CI; save the baseline results.
- [ ] Identify generated files and the scripts that own them. Never hand-edit generated protocol tables if a generator can regenerate them.
- [ ] Confirm the launcher/build flow and determine which parts can run without the original 26.3 Java runtime.

### Milestone 2 — 1.16.5 version data and protocol

- [ ] Obtain verified 1.16.5 metadata and the official 1.16.5 client/server jars for local development. Do not commit Mojang game jars or assets.
- [ ] Extend the version import/generation tools to accept an older target version rather than assuming a forward-only upgrade.
- [ ] Generate version facts and packet IDs from the 1.16.5 protocol data.
- [ ] Implement 1.16.5 handshake/status/login/play packet layouts and state transitions. 1.16.5 predates the modern configuration state and secure-chat protocol, so current state-machine assumptions must be reviewed rather than copied blindly.
- [x] Add an isolated protocol-754 module with handshake, status request/ping, and username-only Login Start frame encoders plus unit tests. This is groundwork only; these encoders are not yet wired into the live connection.
- [ ] Add protocol tests using captured/generated fixtures and verify status ping, offline/local login where supported, and disconnect handling.
- [x] Add isolated protocol-754 Play packet IDs and keep-alive encode/decode helpers with unit tests. This is packet groundwork only; it does not yet implement the Play-state loop.

### Milestone 3 — playable world

- [ ] Adapt registry bootstrap, dimension/biome data, block states, items and entity metadata.
- [ ] Port 1.16.5 chunk sections, palettes, light data, heightmaps and block entities.
- [ ] Adapt movement, interaction, inventory and entity packets to the 1.16.5 layouts.
- [ ] Load the target version's own assets from the user's legally obtained game installation; do not ship game assets in this repository.
- [ ] Add a minimal end-to-end test: connect to a local 1.16.5 server, enter a world, receive chunks, move, and disconnect cleanly.

### Milestone 4 — rendering, compatibility and performance

- [ ] Audit the renderer and shader requirements separately from protocol support. Changing the protocol does not make `wgpu` work on an unsupported GPU/driver.
- [ ] Add a conservative low-end graphics preset (short render distance, reduced effects, capped frame queue) only after the game can render a correct world.
- [ ] Measure frame time, memory use and chunk-load time on representative hardware; avoid claiming performance gains without measurements.
- [ ] Run formatting, unit tests, protocol tests and CI before merging.

## Known technical risks

1. Horizon mirrors many implementation details from the currently pinned vanilla version. A 26.3-to-1.16.5 port is a substantial backport, not just a version-string change.
2. The modern configuration/login flow and packet definitions differ from 1.16.5.
3. Generated data and exporters may depend on Java classes/APIs that do not exist in 1.16.5.
4. The target low-end Intel GPU may fail independently of the protocol port due to graphics API/driver limitations.
5. Rust edition/toolchain and third-party dependencies may have a higher minimum requirement than the target machine can comfortably support.

## Acceptance criteria

The port is not considered playable until the client can negotiate protocol 754 with a 1.16.5 server, enter a world, receive and render chunks, move and interact, and exit without a crash. A successful Rust compilation alone is not sufficient.
