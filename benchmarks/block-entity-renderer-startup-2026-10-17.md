# Block-entity renderer startup, 2026-10-17

Baseline: `64a4c72e` (parent of original optimization commit `392ec447`).
Optimized: `perf/defer-block-entity-render-20261017`, before rebasing onto newer launcher-message commits.
Both use prepared 26.3 resources from `.local/horizon-client/26.3` on the same Linux host.
The host was shared with other, CPU-intensive Rust builds throughout these runs.

`tools/profile_client_startup.py` measures the isolated renderer load via
`examples/block_entity_load_profile.rs`, and optionally measures process launch
through exit after the first title-menu offscreen frame submission. The latter
includes asset loads, GPU initialization, submission and shutdown; it does not
prove GPU completion/presentation. It is **not** a pure renderer timing.

## Isolated loader (seven processes per configuration, milliseconds)

| Build | Registry median (range) | Renderer median (range) |
| --- | ---: | ---: |
| Baseline, debug | 11475.055 (7897.243–23542.118) | 3035.973 (1822.032–4261.591) |
| Optimized, debug | 2972.470 (2828.240–6540.367) | 1036.103 (760.327–1648.694) |
| Optimized, release | 718.290 (588.448–850.662) | 233.104 (163.610–282.917) |

## Controlled paired release-loader comparison

Subsequently, the detached original checkout and optimized checkout each built
the profiling example in release mode. Their binaries had **identical SHA-256**
(`798efedd2acde394a4629377918f9b89923f7e020ab70ef16607bf9476f352cd`).
Fifteen repetitions alternated the invocation order of those copies against
the same resource root:

| Identical release loader executable | Median (range) |
| --- | ---: |
| Built from original checkout | 130.848 ms (125.037–157.920) |
| Built from optimized checkout | 130.590 ms (118.979–158.502) |

The paired optimized-minus-original differences had a median of **-0.557 ms**,
and ranged from -27.330 to +22.451 ms. These are observations of the same
machine code under variable runtime conditions. This controlled run estimates
the deferred renderer work at approximately **131 ms**, consistent with the
initial ~122 ms estimate; it establishes no reduction in loader duration.
The full JSON samples are produced with
`python3 tools/profile_client_startup.py --binary <optimized-example> \
--baseline-binary <original-example> --resources <resource-root> --repeats 15`.

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

The process-level variation is tens of times larger than the expected renderer cost. **No
end-to-end menu speedup is established by these measurements.** The source
change removes the renderer load from the menu path but shifts that load to
world entry. The world-entry path prepares it before rendering the first world
frame, avoiding an additional first-render stall.

## Attempted release-client title A/B

Both original and optimized client executables were built with `--release`.
The updated profiler now creates a fresh temporary game directory for every
variant and repetition, copies the same settings, confirms the client first
frame log marker and records the GPU identity. It also records the separate
`Game::new`-to-first-frame timer, which starts **after** asset loading and is
therefore not a substitute for total process startup duration.

No successful release-client A/B series is available: in two attempts, the
**original** client completed resource loading and identified the NVIDIA RTX
4060 Vulkan adapter, then aborted with `Horizon: Parent device is lost` (exit
code 1), before submitting the first offscreen frame. The profiler correctly
rejected both runs. This repeatable GPU-device failure prevents a reliable
release-menu timing claim; the earlier debug samples remain inconclusive.
The same Vulkan `Parent device is lost` failure also occurred when repeating
an original-checkout **debug world-entry** smoke test, establishing that the
GPU issue is not exclusive to the optimized client.

## Follow-up: controlled software-Vulkan release A/B

The NVIDIA 615.71.09 ICD fails even outside Horizon: `vulkaninfo --summary`
reports `vkCreateDevice failed with ERROR_INITIALIZATION_FAILED`. To isolate
the graphics environment without installing system packages, the Arch
`vulkan-swrast` 26.2.4 package was extracted in `/tmp` and its Lavapipe ICD
JSON modified to point to the extracted absolute library path. With
`VK_DRIVER_FILES=/tmp/horizon-lavapipe/icd.json`, `vulkaninfo --summary`
reports **llvmpipe (LLVM 23.1.1, 256 bits), Vulkan CPU device**.

Fifteen order-alternating release-client pairs, each with a fresh temporary
game directory, same 26.3 resource root, `--frames 1 --offscreen`, and one
llvmpipe adapter. Process timing includes the initial offscreen submission and
client teardown; the separate engine marker begins after asset preparation.

| Title startup, release (15 pairs) | Original | Optimized |
| --- | ---: | ---: |
| Process wall time median | 3317.351 ms | 3183.891 ms |
| Process wall time range | 3237.203–3452.192 ms | 3081.425–3371.159 ms |
| Game construction → first offscreen submission median | 281 ms | 269 ms |

Paired optimized-minus-original process median was **-121.359 ms**; observed
paired differences ranged from **-320.258 to -27.534 ms**, all 15 negative.
This supports a **software-Vulkan title-startup reduction** consistent with
moving ~131 ms of renderer work to gameplay, without claiming a hardware GPU
speedup or a presented-frame timing. Raw data:
`/tmp/block_entity_release_lavapipe_menu.json` on the measurement host.

Seven more release-client pairs used `--world BlockEntityStartupWorld
--test-terrain --render-distance 4 --world-frames 1 --offscreen` to check
the first playable world frame:

| Direct world entry, release (7 pairs) | Original | Optimized |
| --- | ---: | ---: |
| Process wall time median | 4893.770 ms | 4884.942 ms |
| Process wall time range | 4755.564–5318.158 ms | 4834.223–4950.531 ms |
| Game construction → first offscreen submission median | 1595 ms | 1710 ms |

The optimized world-frame marker was **115 ms later** by median, consistent
with the newly deferred loading cost. Total paired process differences had a
median of **+55.425 ms**, range **-435.276 to +109.170 ms**; these mixed values
do not establish a consistent regression in total process-to-world duration.
Raw data: `/tmp/block_entity_release_lavapipe_world.json`. Both experiments
used fully functional software Vulkan, but their offscreen submission markers
do not establish GPU-complete presentation time on discrete NVIDIA hardware.

## Gameplay checks

- Optimized offscreen world-entry smoke: exit 0, 6 frames, 90 generated chunks,
  no error in the smoke report.
- Debug world-entry/reload integration: initial renderer 1011.7 ms, F3+T reload
  1326.4 ms, world live before and after reload.
- F3+T failure/retry with a private linked resource root: deliberately invalid
  `entity-models.json` rejected; the current world kept running and restoring
  the document permitted a successful reload. Initialization was 1090.5 ms,
  successful replacement 953.5 ms (debug build).
- Vanilla chest renderer parity: 100/100 cases matched.
- Renderer slot lifecycle: 2/2 tests passed (concurrent initialization and
  cache replacement after successful or failed initialization).
- Block-entity model tests: 3/3 passed; content-mod tests: 6/6 passed.
- Dedicated-server smoke reached `Done`, ran without creating client renderer
  resources, and was stopped with a 100-second timeout (exit code 124 expected
  for the deliberately long-running server process).
- World-entry telemetry now starts before renderer preparation for local,
  integrated and remote worlds, preventing the deferred work from silently
  disappearing from reported world-opening latency.
- A repeat of the optimized debug failure/retry smoke after the telemetry and
  active-content-mod fixes could not complete: `horizon-ctl wait-ready` returned
  `{"error":"game exited"}` after client GPU initialization. The original
  checkout subsequently failed a comparable world smoke with Vulkan device
  loss. The earlier successful gameplay/reload runs remain evidence, but these
  final end-to-end checks are **environment-blocked**, not claimed as passed.
- Retrying the exact failed-reload/recovery integration on the same latest
  debug client with the working Lavapipe ICD **passed**: first renderer 670.7
  ms, successful F3+T retry 680.9 ms, active world before/after. The earlier
  NVIDIA-specific failure remains documented above.

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
at process startup (`Assets::reload` deliberately reuses them): the reload
path now uses the originally active content-mod selection instead of toggles
pending a restart. Adding/removing content-mod registrations during an active
game still requires a restart.
