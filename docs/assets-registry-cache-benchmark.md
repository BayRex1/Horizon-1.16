# Assets registry cache measurement

The comparison uses `tools/profile_client_startup.py --metric assets`, with
`target/fast/examples/model_library_bench`, the 26.3 resource export, and a
separate writable resource root containing links to original assets and a copy
of `facts.json`. Each invocation runs in a new process. The metric is the time
for `Assets::load`, **including model and sprite work**, rather than isolated
registry decoding or the time to first rendered frame.

| Cache mode | Samples (ms) | Median (ms) |
| --- | --- | ---: |
| Rebuild and persist each run | 12250.262, 2096.747, 1797.825, 2348.889, 1404.190 | 2096.747 |
| Reuse validated cache | 1553.609, 1357.874, 1372.801, 1735.875, 986.098 | 1372.801 |

The observed median was 34.5% lower with a warm cache in this batch. This is
**not an established startup speedup**: the host had concurrent compilation and
large timing variance. Earlier, shorter runs measured cold-cache medians of
788.852 ms (three samples) and 4194.640 ms (five samples), versus warm-cache
medians of 1247.026 ms (five samples) and no matched baseline for the second
batch. Cold runs also include the cost of generating and writing the cache.

To repeat the benchmark, build with
`cargo build --profile fast -p horizon-client --example model_library_bench` and run:

```sh
python3 tools/profile_client_startup.py --metric assets --cold-cache \
  --binary target/fast/examples/model_library_bench --resources RESOURCE_ROOT --repeats 5
python3 tools/profile_client_startup.py --metric assets \
  --binary target/fast/examples/model_library_bench --resources RESOURCE_ROOT --repeats 5
```

Use a writable resource root that is safe to benchmark: `--cold-cache` deletes
`block-registry-cache.nbt` in that root before each sample. A direct registry
stage benchmark on an idle host is needed before claiming a production gain.
