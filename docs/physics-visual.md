# PHYSICS UNLEASHED: visible cube prototype (stage 3)

The opt-in `physics-unleashed` build connects real Rapier poses to Horizon's
existing entity/block renderer. It draws `minecraft:stone` from the user's normal
baked block model and texture atlas. There is no alternate physics animation,
procedural replacement texture, client-side collision solver or bundled game data.

## Download/build

GitHub Actions workflow **PHYSICS UNLEASHED** runs for this branch and pull requests.
After a successful run, download `physics-unleashed-linux-<commit>` from its
Artifacts section, extract the contained `.tar.gz`, and run `sha256sum -c SHA256SUMS`.
It contains Linux development binaries `horizon-client`, `horizon-ctl`, the
falling-cube example and standalone CPU test executables `physics_dimension` and
`physics_visual`. Executable permissions survive in the tar archive. Resources
and a graphics driver are still required to play; binaries alone are not a game
installation. No successful remote Actions run is implied by the workflow source.

To build locally from the repository root with activated Rust stable:

```sh
cargo build --locked -p horizon-client --no-default-features --features physics-unleashed --bins
```

This build has no audio/controllers; omit `--no-default-features` for those on a
machine with the normal system headers. Both server and client need the feature
for this prototype. A feature-disabled client ignores the custom snapshots.

## Real-game check

Use your own prepared Minecraft Java Edition 26.3 resource directory, as described
in the repository README. Start a new creative world with commands enabled. For
example, launch with `--resources /absolute/path/to/26.3 --world PhysicsVisual
--creative`. Follow AGENTS.md's fresh-home rules. In the scratch world only:

```text
/tp @s 12 77 12
/fill 4 71 4 12 96 12 minecraft:air
/fill 4 70 4 12 70 12 minecraft:stone
/physics spawn 8.5 80 8.5
```

Look towards the platform. A one-block stone cube should fall, rebound and settle
with its centre near y=71.5. `/physics gravity 0 9.81 0` should make the same cube
rise. `/physics clear` should remove it on every observing client. Repeat after
joining via Open to LAN; operator permissions are required on the authority.
Join a second feature-enabled client after spawning to check full-snapshot late
join, then clear, change dimension, disconnect/reconnect and check for ghost cubes.
Existing players/mobs still use their vanilla movement; cubes do not block them.

An automated real-game check is included (no hand-written setup code needed):

```sh
python3 tools/physics_visual_smoke.py --bin-dir target/debug --resources /absolute/path/to/26.3
python3 tools/physics_visual_smoke.py --bin-dir target/debug --resources /absolute/path/to/26.3 --in-frame
```

It creates fresh scratch homes, launches through `horizon-ctl`, builds the test
platform, checks actual received body IDs/positions, bounce/rest, antigravity and
clear, requires submitted stone-model vertices, and saves screenshots, pose traces,
control replies and a result report under `.local/physics-visual-smoke/run-*`.
Screenshots still require visual review: submitted vertices are not proof of
visible pixels. `--offscreen` is available where the graphics backend supports it.
Missing resources stop the check with `BLOCKED`/exit 2, never a passing result.

## State and rendering path

- Each authoritative `PhysicsDimension` assigns stable monotonic IDs independent
  of Rapier arena handles, and a new epoch for a new dimension instance. Every
  completed 1/60s step advances its simulation clock. IDs never alias after clear.
- `game/physics.rs::publish_physics` sends `horizon:physics_v1` play CustomPayload
  packets only to placed guests in the same dimension, after simulation and
  commands and during paused server ticks. Full membership supports late joins;
  explicit removal IDs cover clear/unload. Integrated singleplayer uses the same
  real play connection as LAN. In-frame singleplayer consumes the authority's
  same snapshot type directly, without running a second simulation.
- Packets contain dimension, epoch, sequence, physics step, ID, f64 centre position
  and xyzw unit quaternion. The bounded decoder checks sizes, finite values,
  quaternion normalization, duplicate IDs and conflicting removals. Out-of-order
  or other-dimension snapshots cannot resurrect deleted bodies.
- Physics runs 60Hz; snapshots normally arrive at the vanilla server's 20Hz.
  `PhysicsView` buffers up to eight snapshots and renders about 100ms behind the
  latest server time, with position lerp and shortest-arc quaternion slerp. Motion
  is driven by time, not FPS. It holds the last real pose on packet loss, never
  predicts collisions. Frozen duplicate snapshots do not restart the render clock.
- The render transform subtracts the camera in f64, then converts to GPU f32,
  rotates about the real cube centre, and shifts Horizon's [0,1] block geometry by
  -0.5. `block_model_vertices` supplies the ordinary stone atlas/light/normal data
  to the existing depth-tested entity pipeline. No terrain renderer is replaced.
  Rendering is suppressed outside loaded client columns and viewing distance.

## What validation proves

CPU tests cover codec truncation/bad inputs, full membership, stable IDs, actual
Rapier angular motion through the wire/render matrix, 30/60/144FPS interpolation,
removal/epoch/dimension changes, frozen updates and packet loss. A real loopback
TCP test carries the normal compressed Horizon play CustomPayload frame, decodes
it and feeds the client view. Resident-world tests still exercise SceneCollisions.
These are not a running game/LAN session or pixel evidence.

In this cloud environment there were no prepared Minecraft resources, no DISPLAY
or WAYLAND_DISPLAY, and no GPU device. The implementation must not be described as
a visually verified game demo until the above game check runs and its screenshots
are reviewed. An actual startup of the compiled client with `--offscreen` exited
with Winit's `neither WAYLAND_DISPLAY nor WAYLAND_SOCKET nor DISPLAY is set`
error before loading a world. The smoke runner separately reported missing
prepared resources (exit 2). Do not publish generated resources, screenshots containing private
data, jars or game exports in the source repository.

Remaining limits: no persistence, no player-body collisions, no physics-specific
shadow caster, no command autocomplete or cross-version negotiation. Snapshot
delivery/interpolation adds latency; supported bodies remain up to 16 unit stone
cubes per loaded dimension. Chunk-neighbour unload despawns bodies as in stage 2.
The next steps are in-game/LAN visual validation, then dedicated rendering tests,
snapshot bandwidth tuning and interaction with physics bodies.
