# PHYSICS UNLEASHED: physics-core

Headless, opt-in rigid-body simulation in Horizon's root Rust workspace. The
`horizon-client/physics-unleashed` feature connects it to resident dimensions;
vanilla player/mob movement is unchanged. See the
[stage 2 integration guide](../../docs/physics-unleashed.md) for commands and limits.
The [stage 3 guide](../../docs/physics-visual.md) describes the renderer and network snapshots.
No Minecraft files, Java, GPU, renderer or background service are needed here.

## Run

From the repository root, with Rust stable (validated on 1.99.1):

```sh
cargo run --locked -p physics-core --example falling_cube
cargo fmt --all -- --check
cargo check --locked -p physics-core --all-targets
cargo test --locked -p physics-core
cargo clippy --locked -p physics-core --all-targets -- -D warnings
```

The example creates a real fixed platform and a dynamic cube, runs 180 steps,
prints CSV positions/velocities, and exits with an error if no rebound occurs.
Tests additionally check dynamic/dynamic momentum exchange, a 1000m/s CCD impact,
force persistence/replacement, impulse mass scaling, off-centre angular response,
gravity changes waking sleepers, exact time accumulation, block-shaped collisions,
removal and invalid inputs. These are simulation checks, not a rendered game demo.

## Contracts

- Rapier **rapier3d-f64 0.26.1** is pinned in Cargo.toml and Cargo.lock. Its f64
  coordinates match `horizon-core::geometry` without float narrowing; its nalgebra
  0.33 API builds with the existing toolchain. No engine-wide math/toolchain
  replacement is needed. `enhanced-determinism` is enabled; parallel/SIMD feature
  flags are not enabled. This is not a promise of multiplayer lockstep across all
  CPUs/platforms; networking must use authoritative snapshots.
- One block is one metre, Y is up, time is seconds, mass is kg, force is N,
  impulse is kg m/s. Default gravity is `(0, -9.81, 0)` and can be changed or zeroed.
  Density defaults to 1kg/m³ as a neutral test material, not Minecraft balance.
- `step()` always advances 1/60s. `advance(Duration)` keeps an integer accumulator
  in nanoseconds × 60: 50ms produces exactly three steps, independent of render
  frame partitioning. At most eight steps run per call; backlog is retained and
  exposed by `pending_steps()`, and can be drained with `advance(Duration::ZERO)`.
  Do not feed paused wall time or mix the two stepping modes for the same time.
- `set_force` replaces the persistent centre-of-mass force, which acts on every
  subsequent substep until cleared with zero. Impulses act once and wake a body;
  a world-space off-centre impulse also changes angular velocity. Gravity changes
  wake dynamic bodies. Finite input, positive extents/density and material ranges
  are checked before insertion. Handles are generational and scoped to one world.
- A construction is one rigid body with cuboid colliders derived from each
  `BlockCollider`'s block-local `VoxelShape::boxes()` and integer `BlockPos` offset.
  Position/rotation describe the body's original origin, not its centre of mass.
  Empty block shapes are skipped; an all-empty construction is rejected. Parts
  must not overlap for correct density-based mass/inertia. Block state IDs,
  materials and destruction topology remain the caller's responsibility.
- Hard CCD is enabled for dynamic bodies, with up to four internal CCD substeps
  inside the fixed 1/60s interval. Rapier intentionally uses discrete contacts
  below its CCD threshold; transient overlap within a step is possible. The drop
  test bounds it by per-step travel, requires rebound/rising motion, and rejects
  crossing the platform. The separate high-speed test exercises CCD itself.

## Real Horizon integration points

1. `crates/horizon-client/src/game/levels.rs`: `Parked` stores per-dimension state,
   and `Game::tick_server` swaps/ticks levels. `World.physics` now keeps a separate
   engine in this lifecycle behind an off-by-default feature. In
   `game.rs::tick_world(running)`, only an authoritative running world advances;
   at the normal 20Hz tick rate, 50ms means three physics steps. Respect freeze,
   step, sprint and changed tick rates in `clock.rs`; do not also simulate on a
   remote client's render loop or share a world between dimensions.
2. `world.rs::World::block`/`World::set` and `World::block_log` provide loaded block
   state and edit notifications. `collisions.rs::SceneCollisions` implements
   `horizon_world::block_collision_scan::BlockCollisionWorld`; its `context_shape`
   returns the selected block-local `VoxelShape` used by this crate's adapter.
   Preserve contextual/uncached shapes, chunk availability and errors. A
   `BlockCollisionScan` hit is already world-translated: do not translate it twice
   or treat an unavailable shape/chunk as a solid cube or as air.
3. The stage 2 adapter builds fixed colliders for loaded terrain near active
   bodies and updates them on edits/unload without consuming the network block
   log. Existing blocks are preserved. Stage 3 publishes stable-ID snapshots and
   interpolates transforms in the existing block renderer.

The core supplies the CPU foundation and shape adapter. Live terrain sync lives
in `horizon-client::physics`; rendering/snapshots are in `physics_view` and the game
adapter. Player/body interaction, save/load, fracture,
joints, Physics Gun or force anomalies. Game/LAN/parity validation still needs
the user's own prepared Minecraft 26.3 resources. Before enabling gameplay,
test server-thread, dedicated and in-frame paths and keep vanilla movement
resolvers (`collisions.rs`, `horizon-world::world_movement`) intact.
