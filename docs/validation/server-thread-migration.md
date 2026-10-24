# Server-thread integration coverage migration: validation ledger

Tested on 2026-10-24, Linux / NVIDIA Vulkan, with Java 25 and the pinned 26.3
resources and jars. Base: `e423e570c`. Player integration uses real Horizon
clients under Xvfb and real control sockets. The migrated game drivers use fresh
temporary game, launcher and XDG data/config homes; telemetry, audio and launcher
self-update are disabled. No player data or prepared resources are committed.
An interrupted historical-world generator initially inherited the harness
environment instead of those per-game homes. It wrote only to ignored `.local`
fixture folders; it was stopped, and subsequent validation uses the already
prepared historical worlds. That setup mistake is not counted as a passing run.

## Before editing

Open PR searches for server-thread, integration, LAN and friends work found no
overlapping migration. The full original LAN suite was started with
`--server-thread` against a build of the unchanged base before repository edits.

| Run | Hosting | Result |
| --- | --- | --- |
| Original full LAN attempt | Server thread | 58 checks reported: 39 passed, 18 failed, deflection skipped; the runner then aborted at `light` with an uncaught `TypeError`. |
| Original remaining 45 checks | Server thread | 31 passed, 14 failed, no skips. Used the unchanged loaded driver and original executable. |
| Friends, including `--baseline` | Server thread | Plain LAN, direct invitation, local relay and ended-invite rejection passed. |

Together the two LAN invocations attempted all 103 original checks. The initial
`sign` case left its editor open, breaking the later death-screen assertion.
`predict` suspended the whole host process; resuming it interrupted a socket
read (`Interrupted system call`), disconnected the guest and caused later
session failures. The remaining run also exposed sign placement outside reach
and cascading dead-player failures. These are recorded failures, not passes.

## Infrastructure changes validated during migration

- Close the sign editor after the original placement check. Give the two-player
  sign case a reachable arena and wait for placement data/feedback before typing.
  Observe the submitted text on both clients within the existing assertion budget.
  The revised sign case passed three consecutive server-thread reruns.
- Release held input, close screens and respawn dead players before independent
  cases. Keep every case's death, menu and held-action assertions inside the case.
- Replace process signals with a server barrier for prediction and an intentional
  window-thread stall for frame-recovery tests. Prediction still tests immediate
  client prediction, authoritative agreement, iron-door rejection and exactly-once
  sound; it also asserts that the server clock did not advance at the barrier.
- Route mob/projectile/hurt-log inspection to the owning server. Deflection runs
  in the default suite rather than reporting an in-frame-only skip as a pass.
- Record skipped checks explicitly as uncovered, and record ordinary case errors
  without aborting the rest of the suite. Write mode and results to `results.json`.
- Serialize the two CI integration entry points on the shared GPU. An earlier
  concurrent comparison failed to start a graphics device because of GPU memory
  exhaustion; it was rerun serially, without changing a gameplay timeout.

## Post-migration runs

The first complete server-thread migration run, before the case-cleanup fixes,
ran all 103 original cases: 88 passed and 15 failed. Failures were `deflect`,
`light`, `swings`, `cracks`, `effects`, `portals`, `flight`, `boats`, `minecarts`,
`frames`, `hands`, `riding`, `mounts`, `contacts` and `absorption`. Many followed
the host's deflection damage/death. They are not used as evidence of a clean suite.

The next complete default server-thread run executed all 107 cases (the original
103 plus four separate host/guest sensor and dispenser input cases): **97 passed,
10 failed, zero skipped**. Failures were `commands`, `deflect`, `swings`, `flight`,
`boats`, `minecarts`, `keepinv`, `fishing`, `sculk_input_host` and
`sculk_input_guest`. Lighting passed this run; earlier lighting failures are not
claimed as a persistent blocker.

A subsequent input-timing correction runs the existing double-jump key/wait
sequence through one `horizon-ctl script` process, preserving the seven-tick
flight window. A targeted server-thread rerun passed flight for both players,
swings, boats and minecarts. Command completion still failed: the observable
suggestion list did not appear within the existing two-tick budget. The in-frame
rerun passed flight and failed completion in the same way. Passing isolated
swings/vehicle cases does not clear the unexplained failures in the full sequence.
The full-run inventory retention and fishing failures also remain unresolved.

Friends passed plain LAN, direct invitation, local relay and ended-invite checks
both after making server-thread hosting the default and with explicit `--in-frame`.

Targeted in-frame comparisons passed chest, sign placement, prediction, clock
recovery, lighting and deflection. The revised two-player sign case and both
new dispenser input cases passed too. Command completion failed twice: Tab after
`/gamem` left `/gamem` unchanged. The host's sculk input passed; the guest's failed.

Targeted server-thread checks passed prediction and both new dispenser input
cases. Dispensing checks authoritative inventory consumption and exactly one
matching item on each client. Both sensor cases confirmed movement on the client
and server, but the sensor stayed inactive. Deflection still failed the original
no-fireball-damage assertion after allowing the click through a normal client
input tick. An experimental frozen-world synchronization approach also failed
and is not part of the final implementation.

## Deterministic rule and source checks

`python3 tools/parity.py command_exec dispenser sculk --timeout 1500 --jobs 2`:

| Direct fixture | Result |
| --- | --- |
| Command execution | 781/781 command/callback records match the vanilla oracle. |
| Dispenser and related rule calls | 6000 cases match; no difference groups. |
| Sculk | 435 scenarios match every compared tick. |

All three harnesses passed. Their seeded rule calls and controlled ticks remain
direct; they do not claim client/server boundary coverage. The existing
`net_harness.rs` remains a real dedicated-server client fixture.

The first quick self-check passed 289 Python tests, rustfmt and Clippy. Rust tests
reported 1081 passed, one failed and 26 ignored: the shared prepared resource
folder lacked `registries.json`. The pinned local datagen report was copied into
a private ignored resource copy; the failed
`mod_content::tests::content_root_loads_with_every_table` test then passed.
The focused environment/parity-runner/self-check Python tests passed (38 tests).

The full `python3 tools/pr_ready.py --base origin/main` completed: 1205 Rust tests
and 291 Python tests passed, as did rustfmt, Clippy and documentation checks.
Its parity phase failed after the historical-world setup interruption described
above, so the overall self-check is **not green**. A subsequent `parity.py --changed origin/main --timeout 1500` selected 160
harnesses, 151 requiring runs. Three completed and passed (`firework_tooltip`,
`held_drop`, `interaction_input`). Both integration wrappers exceeded that
invocation's 25-minute per-harness deadline: friends was waiting for the shared
GPU lock, and LAN was still executing cases. Automatic retries were stopped and
the detached game processes cleaned up. The remaining affected harnesses did not
complete. This is incomplete validation, not a passing corrected parity gate;
compilation is not a substitute for the gameplay failures above.

The final source self-check, `python3 tools/pr_ready.py --quick`, passed 1205 Rust
tests, 291 Python tests, rustfmt, Clippy and documentation checks. After correcting
the integration build arguments to name each binary (so traced selection includes
`game.rs` and `game/integrated.rs`), the named client/control/relay build and 38
focused Python tests passed. Dependency extraction from the real Cargo dep-info
confirmed both game files are selected.

## Confirmed limitations

This migration is not ready to merge while required coverage remains red.

- Server-player footsteps do not reach sculk sensors. `tick_entity_events` in
  `game/sculk.rs` emits player motion events from `Game.player`; the integrated
  server's players live in its guest hub. The in-frame host passes and its guest
  fails too, separating this shared server-player gap from the deterministic rule
  oracle. A fix needs per-player event identity/state rather than weakening the test.
- The server-thread host takes fireball damage in deflection; the equivalent
  in-frame check passes. Client-visible and authoritative projectile state diverge
  during the synchronization investigation. No gameplay fix is included.
- Command completion fails in both modes as described above; it is not reported
  as a passing fallback comparison.
- Full-sequence swings, vehicle, inventory-retention and fishing failures need
  further fixture/state investigation. Swings and both vehicles passed isolated
  reruns; that evidence does not establish a gameplay defect or a full-suite pass.

No failing case is skipped, expectation relaxed, or existing timeout increased to
hide these results. The PR stays draft and reports the remaining coverage blockers.
