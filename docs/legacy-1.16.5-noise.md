# Minecraft Java 1.16.5 legacy Nether noise

This note records the version-specific noise path implemented in
`horizon-world::legacy_seeded_noise_1_16_5`. It is deliberately separate
from the post-1.18 positional/Xoroshiro noise implementation.

## Evidence and version boundary

- Yarn's pinned 1.16.5 documentation exposes the legacy
  `OctavePerlinNoiseSampler`, `DoublePerlinNoiseSampler`, and
  `ChunkRandom` APIs:
  - <https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/util/math/noise/OctavePerlinNoiseSampler.html>
  - <https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/util/math/noise/DoublePerlinNoiseSampler.html>
  - <https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/world/gen/ChunkRandom.html>
- Cubiomes documents its Nether generator as Minecraft 1.16+ and independently
  implements the legacy Java LCG, Perlin octave initialization, DoublePerlin
  composition, and Nether seed setup:
  - <https://github.com/Cubitect/cubiomes/blob/master/rng.h>
  - <https://github.com/Cubitect/cubiomes/blob/master/noise.c>
  - <https://github.com/Cubitect/cubiomes/blob/master/biomenoise.c>

The Cubiomes source is used as an independent algorithm cross-check, not copied
into Horizon. Yarn confirms the public 1.16.5 API/version surface; the exact
numerical implementation still requires a pinned executable vanilla parity
fixture before claiming bit-for-bit world-generation parity.

## 1.16.x Nether configuration

The built-in Nether climate source uses two DoublePerlin fields:

| Field | World seed | First octave | Amplitudes |
| --- | ---: | ---: | --- |
| Temperature | `seed` | -7 | `[1, 1]` |
| Humidity | `seed + 1` | -7 | `[1, 1]` |

Altitude and weirdness are constant zero for the built-in Nether preset; they
are not additional independently seeded Nether noise fields. Biome selection
samples the quart-coordinate grid and uses y=0.

## Octave construction

For first octave `-7` and two amplitude slots, the highest represented octave
is `-6`. The legacy random stream first skips the six absent octaves between
octave 0 and octave -6. Each missing octave advances Java's 48-bit LCG by
exactly 262 state transitions. The two represented Perlin samplers are then
constructed from highest to lowest frequency:

| Constructed layer | Octave | Frequency | Normalized weight |
| --- | ---: | ---: | ---: |
| First | -6 | `2^-6` | `1/3` |
| Second | -7 | `2^-7` | `2/3` |

A zero amplitude slot is still significant to the parameter layout and
consumes the same 262-step skip rather than constructing a Perlin sampler.
The Java Random constructor itself consumes three `nextDouble` values and
the 256-entry Fisher-Yates permutation draws.

## DoublePerlin composition

The first and second octave stacks are constructed sequentially from the same
Java Random stream. The second stack samples a transformed domain using
`337 / 331`. For two adjacent non-zero amplitude slots, the output scale is
`10 / 9`; the general normalization is derived from the first and last
non-zero amplitude positions.

## Compatibility limits

- These modules implement the legacy Nether climate noise path only.
- This does not complete Overworld terrain density, caves, surface rules,
  feature placement, or chunk generation.
- The algorithm and constants have independent source support, but a
  version-pinned vanilla oracle run is still required to certify numerical
  parity for selected seeds and coordinates.
