# Opening timing

The shared opening defaults use a 160 ms hold, a 400 ms readiness timeout,
and a 240 ms reveal. This gives the launcher and game a brief introduction
without the former 1.4–2.5 second hold and 800 ms reveal.

Both production transition paths still wait for readiness until the timeout
(launcher release list; game backdrop). The launcher retains its persisted
skip setting and focused click/key skip handlers. The game retains its
click/key skip handlers, once-per-session opening, and classic-style and
frame-harness bypasses. No input, focus, easing, or resource-loading code changes.

## Reproduce the timing comparison

From the repository root:

```sh
python3 tools/check_opening_timing.py 73a019967e9665c6b96332988c13cb66679d8477
```

The harness compiles the actual launcher `opening_phase`/`skip_opening`
methods and game `Opening` and `Title` transition methods from each revision.
It substitutes a virtual `Instant` and minimal GUI containers, steps at
60 Hz, and compares the same readiness/skip inputs. One warmup and 31 runs
per case use `black_box`; assertions check monotonic progress, completion,
readiness/timeout timing, input skip timing, and the launcher's disabled opening.

Measured on Linux 7.2.8, rustc 1.99.0, Python 3.14.7. Median simulated time
to fully revealed controls, in milliseconds (both paths have identical results):

| Readiness / input | Before | After | Saved | Before / after |
| --- | ---: | ---: | ---: | ---: |
| Ready on first frame | 2,200 | 416.667 | 1,783.333 | 5.28× |
| Ready at 500 ms | 2,200 | 650 | 1,550 | 3.38× |
| Never ready during opening | 3,300 | 650 | 2,650 | 5.08× |
| Click/key skip at 100 ms | 900 | 350 | 550 | 2.57× |
| Launcher skip setting | 0 | 0 | 0 | — |

The ideal continuous-clock ready duration is 400 ms (160 + 240); frame
quantization yields 416.667 ms. This measures complete reveal, a conservative
control-readiness milestone: game controls can accept input during the reveal.
It measures deterministic animation delays, not CPU throughput, resource-load
time, overall process startup, or FPS. No GUI wall-clock or visual validation
was performed; real frames may arrive later. An unfinished release list or
backdrop can remain unfinished after the earlier timeout, as before. There
is no additional production allocation or memory tradeoff.
