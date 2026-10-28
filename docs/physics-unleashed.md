# PHYSICS UNLEASHED: resident-world integration (stage 2)

This is an experimental, CPU-side integration. It creates real Rapier bodies in
the authoritative Horizon world and collides them with the world's selected
block shapes. It does **not** draw bodies, send them to clients or save them.
Vanilla player/mob movement and block contents are unchanged.

## Enable and inspect

```sh
cargo build --locked -p horizon-client --no-default-features --features physics-unleashed --bin horizon-client
```

The feature is off by default. The above development binary has no audio or
controller dependencies; it still needs the usual prepared Minecraft 26.3
resources to start a game. Use `--features physics-unleashed` with default features
for an audio/controller build on a machine with the documented system headers.
Follow AGENTS.md's scratch-home rules for every game run.

In a loaded world, as an operator (level 2 or above), enter absolute coordinates:

```text
/physics spawn 8.5 80 8.5
/physics status
/physics gravity 0 9.81 0
/physics gravity 0 -9.81 0
/physics clear
```

Pick a position above existing terrain in a fully loaded area. `spawn` creates a
unit cube with restitution 0.6 without removing any blocks. `status` reports each
body's actual Rapier position, rotation and velocity. Commands run on the server
(including the integrated server); remote clients do not simulate a second copy.
There is no command-tree registration/autocomplete yet. The cube is invisible:
status text is diagnostic output, not a rendered physics effect.

## Ownership and scheduling

With the feature enabled, each `world::World` owns a `PhysicsDimension`. `Parked`
in `game/levels.rs` stores and swaps the complete World; parked dimensions thus
retain independent bodies, gravity, terrain cache and time remainder. Closing a
dimension drops its Rapier sets. An in-frame dimension switch creates a new World
and therefore discards the old transient bodies, as opposed to persisting them.

`game.rs::tick_world` calls `tick_physics` after authoritative world/block/entity
updates. Remote clients return via `tick_remote` before this path. A running
vanilla tick feeds 50ms of **simulation** time, giving three fixed 1/60s steps at
the normal 20Hz rate. Changed vanilla tick rates/sprinting speed up simulation time
accordingly, while freeze/step respect `world_runs`. No wall-clock lag is added.
The adapter also caps arbitrary elapsed input to eight steps per call, discards
excess whole-step debt and retains only the fractional remainder. This bounded
policy is intentionally different from physics-core's general-purpose `advance`.

## Loaded terrain, block edits and safety limits

Before each substep the adapter queries `BlockCollisionScan` over each cube's
swept bounds plus a two-block margin. The scan uses the real `SceneCollisions`,
`BlockPos`, block state registry and `VoxelShape` selection. Shapes of stairs,
slabs and other blocks are not replaced with decorative/full-block approximations.
Scan results are world-translated, and the adapter converts them back to local
coordinates exactly once before inserting fixed block bodies.

The terrain working set is keyed by block position; matching box geometry reuses
the existing collider. Changed forms replace it, removed blocks and unneeded
positions remove it. Empty shapes produce no collider. Contextual geometry is
queried again even without a block write (e.g. a moving piston). The rigid cube
uses `CollisionContext::empty(false)` with no player equipment/entity abilities;
this defines this stage's rigid-body collision policy, not player collision parity.

`World::block_log` is a network-owned optional/drained log, so physics deliberately
does not consume it or depend on it being enabled. Re-querying the bounded working
set catches `World::set`, neighbour-dependent shape changes, direct chunk updates,
loaded/replaced chunks and changes after the network has drained its log. No
unbounded second edit log is retained. Colliders are demand-loaded near bodies,
not allocated for every distant loaded chunk or every air cell.

Before scanning, the complete swept chunk footprint (including scan border cells)
must be resident. Missing chunks never become air: the affected body is despawned
and its terrain cache is pruned, including on zero-duration/frozen adapter calls.
After reloading, a new body can be spawned. There is no automatic respawn or disk
persistence. Unknown/failed shapes stop the adapter before a simulation step;
the game logs the error and continues vanilla ticking. Limits: 16 cubes per
dimension, 32,768 scan candidates per cube/substep, 16,384 cached terrain blocks,
64 chunk columns per sweep, gravity components in +/-100m/s². Bodies beyond the
world's vertical extent are despawned. No chunk tickets are forced by physics.

## Validation and artifacts

```sh
cargo fmt --all -- --check
cargo test --locked -p physics-core
cargo check --locked -p horizon-client --no-default-features --features physics-unleashed --lib --bins
cargo test --locked -p horizon-client --no-default-features --features physics-unleashed --test physics_dimension
```

`physics_dimension` exercises real World/Chunk storage and SceneCollisions using
a small, authored registry fixture: falling/bouncing, slab replacement, removal,
network-log preservation, unload/reload, neighbouring chunk availability, gravity,
dimension ownership, unknown shapes, pause and bounded lag. These automated CPU
integration tests do **not** prove a game session, LAN commands or rendering work.

The `PHYSICS UNLEASHED` GitHub Actions workflow builds/tests the core and affected
Horizon paths, checks feature-disabled compilation, and uploads development Linux
binaries, the standalone `physics_dimension` test executable, a falling-cube CSV,
commit ID and SHA-256 checksums for seven days after successful checks. Download
the artifact, extract its `.tar.gz` (preserving executable permissions), run
`sha256sum -c SHA256SUMS`, then `./physics_dimension --nocapture` on Linux to repeat the CPU integration tests
without Minecraft resources. The artifact contains no Minecraft resources. Workflow source
and local checks do not establish a successful remote Actions run.

## Next step: an actual cube on screen

Publish authoritative per-dimension body snapshots with stable network IDs,
remove notifications on unload/despawn, and interpolate previous/current rigid
transforms in the client renderer. Add a cube/block-construction mesh draw using
those transforms. Validate the integrated-server, dedicated/LAN and in-frame
paths in a real game with prepared Minecraft resources. Save/load, player-body
interaction, fracture and destruction are separate later steps.
