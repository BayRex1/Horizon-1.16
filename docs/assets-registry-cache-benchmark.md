# Assets registry startup: compatibility and benchmark review

## Implementation and compatibility

`Assets::load` still reads and parses `facts.json` and validates the pinned
resource manifest. On a cache miss it performs the original recursive
JSON-to-NBT conversion, then calls `BlockRegistry::from_nbt`. The precomputed
cache stores an ordered, versioned binary description of each validated block
state and its properties, followed by default IDs. On a cache hit,
`BlockRegistry::from_parts` reconstructs the registry while checking duplicate
states, default references, and complete block-default coverage. The state
vector remains in original ID order. The internal state-to-ID lookup uses a
`HashMap` instead of a `BTreeMap`; it does not define state order or IDs.

The cache format is versioned independently of the resource data version.
Each cache records SHA-256 of the complete `facts.json` and SHA-256 of the
payload; the decoder checks the data version, bounds, UTF-8, duplicates,
trailing bytes, and registry consistency. Missing, malformed, stale, oversized
or corrupt caches fall back to the original conversion. Files are written via
unique `create_new` temporary files, synced, and atomically renamed in the
same directory, with best-effort directory sync. Write failures do not prevent
normal startup. Concurrent writers publish complete cache files, and any
result from a stale source is rejected by the next hash check.

Content-mod overlays under `cache/mod-content` are rebuilt each launch, so
their registry cache is kept in the sibling
`cache/mod-content-block-registry-cache.bin`. Changes to the overlay's
`facts.json` invalidate it. Model and resource-pack reloads reuse the existing
registry through `Assets::reload`; their assets are not cached here.

The focused tests compare each cached state, ID, property and default against
the original NBT path and cover invalid numbers, duplicate states, invalid
defaults, missing and malformed facts, cache tampering and truncation,
read-only roots, cache-write failures, concurrent writers, and mod-overlay
rebuilds. A separate integration test loads actual content-mod resources.

## Controlled measurements

Both binaries are from the same pinned base commit `f535d1b4` (the optimised
build adds the PR changes), built with the **same Cargo `fast` profile**.
Both run against the same Minecraft 26.3 resource export via an isolated,
writable root: top-level resources are symlinks and `facts.json` is a private
copy. The base build uses the original registry loader; the candidate uses
the versioned cache and hash lookup map. Each sample is a fresh process. The
profiler excludes process startup overhead before `main`, GPU initialization,
game/UI initialization, and time to first rendered frame.

The final measurements below were made with no `cargo` or `rustc` processes
visible at the benchmark boundaries. Repeats are sequential, so these are
not CPU-isolated system-wide benchmarks. A first-run outlier in an earlier
batch underscores variability even without compilation.

**Final full loader batch, 10 samples per condition:**

| Condition | Assets median | Assets + items median | Total MAD | Total range |
| --- | ---: | ---: | ---: | ---: |
| Original, no registry cache | 476.286 ms | 517.266 ms | 11.766 ms | 491.844–542.810 ms |
| Optimised, warm validated cache | 393.635 ms | 428.983 ms | 4.806 ms | 420.476–484.749 ms |
| Optimised, rebuild cache each run | 569.404 ms | 609.638 ms | 14.628 ms | 575.214–715.752 ms |

The warm candidate's total median is **17.1% lower** than the original in
this batch, whereas creating a new cache is **17.9% slower**. In the earlier
batch before the `HashMap` change, medians were 515.164 ms original, 456.872
ms warm, and 559.093 ms cold; that build is not interchangeable with the final
one. Ten baseline runs in that batch included an initial outlier of 2074.073
ms. Results should be judged on repeated runs, not one launch.

**Registry-only microbenchmark**, 20 alternating samples, using the *final*
lookup-map implementation on every path. Source JSON was parsed before the
clock started, and the return value was consumed using `black_box`:

| Registry path | Median | Range |
| --- | ---: | ---: |
| Original JSON-to-NBT conversion and registry construction | 402.985 ms | 385.893–439.972 ms |
| Decode and validate warm typed cache | 435.357 ms | 417.972–476.418 ms |
| Convert, validate and persist new typed cache | 703.097 ms | 681.642–757.870 ms |

**Interpretation:** The cached binary representation does **not** demonstrate
a registry-conversion speedup: a warm hit is about 8% slower than conversion
when using the same lookup implementation. The full-loader improvement over
the base build arises primarily from changing the internal state-ID lookup
construction to a hash map. The cache is useful as a validated representation
and maintains compatibility, but its disk I/O, checksum, decoding, and
allocation costs remain measurable; treating its presence alone as a
performance win would be inaccurate. This aspect merits further work before
claiming the precomputation itself is a startup optimisation.

## Reproducing

Build the baseline commit and candidate commit in separate worktrees with:

```sh
cargo build --profile fast -p horizon-client --example model_library_bench
```

Run each from the candidate worktree with the same writable `RESOURCE_ROOT`:

```sh
python3 tools/profile_client_startup.py --metric startup \
  --binary /PATH/TO/BASELINE/target/fast/examples/model_library_bench \
  --resources RESOURCE_ROOT --repeats 10
python3 tools/profile_client_startup.py --metric startup --cold-cache \
  --binary target/fast/examples/model_library_bench \
  --resources RESOURCE_ROOT --repeats 10
python3 tools/profile_client_startup.py --metric startup --warm-cache \
  --binary target/fast/examples/model_library_bench \
  --resources RESOURCE_ROOT --repeats 10
```

The profiler reports raw samples, median, median absolute deviation, minimum
and maximum for `block_ms`, `item_ms`, and `total_ms`. `--cold-cache` removes
the derived cache before **each** sample, so never use it on a resource root
whose cache you need to preserve. The ignored Rust microbenchmark can be run
from a compiled library test executable with `HORIZON_REGISTRY_BENCH_FACTS`
pointing to the source `facts.json` and the filter
`benchmark_registry_conversion_only --ignored --nocapture`.

The primary limitation is that these measurements cover resource-dependent
asset and item preparation, not full client startup or the first rendered
frame. They also do not measure long-term OS cache effects, disk durability
under simulated power loss, or two independent processes racing to rebuild a
content-mod overlay itself.
