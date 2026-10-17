# Block-entity renderer startup, 2026-10-17

Baseline: `64a4c72e` (parent of original optimization commit `392ec447`).
Optimized: `perf/defer-block-entity-render-20261017`, before rebasing onto newer launcher-message commits.
Both use prepared 26.3 resources from `.local/horizon-client/26.3` on the same Linux host.
The host was shared with other, CPU-intensive Rust builds throughout these runs.

`tools/profile_client_startup.py` measures the isolated renderer load via
`examples/block_entity_load_profile.rs`, and optionally measures process launch
through exit after the first title-menu frame. The latter includes asset loads,
GPU initialization, rendering, and shutdown. It is **not** a pure renderer timing.

## Isolated loader (seven processes per configuration, milliseconds)

| Build | Registry median (range) | Renderer median (range) |
| --- | ---: | ---: |
| Baseline, debug | 11475.055 (7897.243–23542.118) | 3035.973 (1822.032–4261.591) |
| Optimized, debug | 2972.470 (2828.240–6540.367) | 1036.103 (760.327–1648.694) |
| Optimized, release | 718.290 (588.448–850.662) | 233.104 (163.610–282.917) |

Baseline debug renderer samples: `[1926.100, 3035.973, 3234.572, 4261.591, 2712.252, 3044.630, 1822.032]`.
Optimized debug renderer samples: `[1648.694, 1036.103, 1066.644, 760.327, 1068.459, 858.276, 902.386]`.
Optimized release renderer samples: `[163.610, 187.061, 282.917, 261.064, 253.352, 178.317, 233.104]`.

The renderer loader itself was not changed. Differences between baseline and
optimized *isolated loader* timings therefore indicate host contention, cache
effects, and measurement variability; they are **not** optimization gains.

## First title-menu frame (debug binaries, Xvfb, RTX 4060)

Five paired repetitions, alternating baseline-first and optimized-first.
Each process used `--frames 1 --offscreen` and the same isolated game directory.

| | Median wall time | Range |
| --- | ---: | ---: |
| Baseline | 23677.254 ms | 22536.678–64471.618 ms |
| Optimized | 33915.799 ms | 21716.549–57491.279 ms |
| Paired optimized minus baseline | +1463.545 ms | -6980.339–+12500.739 ms |

Baseline samples: `[64471.618, 22536.993, 22536.678, 25521.545, 23677.254]`.
Optimized samples: `[57491.279, 33915.799, 21716.549, 26985.090, 36177.993]`.
Paired differences: `[-6980.339, 11378.806, -820.129, 1463.545, 12500.739]`.

The variance exceeds the expected renderer cost by orders of magnitude. **No
end-to-end menu speedup is established by these measurements.** The source
change removes the renderer load from the menu path but shifts that load to
world entry. The world-entry path prepares it before rendering the first world
frame, avoiding an additional first-render stall.

## Gameplay checks

- Optimized offscreen world-entry smoke: exit 0, 6 frames, 90 generated chunks,
  no error in the smoke report.
- Debug world-entry/reload integration: initial renderer 1011.7 ms, F3+T reload
  1326.4 ms, world live before and after reload.
- Vanilla chest renderer parity: 100/100 cases matched.
- Renderer slot lifecycle: 2/2 tests passed (concurrent initialization and
  cache replacement after successful or failed initialization).
- Block-entity model tests: 3/3 passed; content-mod tests: 6/6 passed.

## Reproduction and limits

Build `--example block_entity_load_profile` and the client binary separately
for the original parent checkout and the optimization branch. Invoke
`tools/profile_client_startup.py --binary <example> --resources <prepared-root>
--repeats 7` for the isolated load. For title timing, use
`xvfb-run -a python3 tools/profile_client_startup.py --repeats 0
--client-binary <optimized-client> --baseline-client-binary <baseline-client>
--resources <prepared-root> --client-cwd <scratch-game-dir> --client-repeats 5`.
Use matching profiles and stable host/GPU conditions for meaningful comparisons.

Pack reload rebuilds the renderer for the active resource root and retains the
previous generation if preparation fails. Registries and block facts are fixed
at process startup (`Assets::reload` deliberately reuses them): adding/removing
content-mod registrations during an active game still requires a restart.
