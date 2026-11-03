# Minecraft Java 1.16.5 noise API reference

This note records version-pinned public API facts used by the clean-room Rust
port. It is not an implementation specification: Yarn Javadocs expose class
members and signatures, but generally do not publish method bodies or the
complete order of random-number draws.

## Version pin

- Minecraft Java Edition: 1.16.5
- Yarn: `1.16.5+build.5`
- Official Fabric Maven Javadocs: `https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/`

## Confirmed API surface

### `PerlinNoiseSampler`

Reference:
https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/util/math/noise/PerlinNoiseSampler.html

The class exposes three coordinate origins (`originX`, `originY`,
`originZ`), a private permutation byte array, and a constructor accepting
`java.util.Random`. Its public sampling overloads are:

- `sample(double x, double y, double z, double, double)`
- `sample(int sectionX, int sectionY, int sectionZ, double localX, double localY, double localZ, double fadeLocalX, double fadeLocalY, double fadeLocalZ)`

The class also declares a private gradient lookup and a static gradient
function. The Javadocs do not specify the constructor's exact random draw
sequence or the arithmetic inside either sampling overload.

### `OctavePerlinNoiseSampler`

Reference:
https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/util/math/noise/OctavePerlinNoiseSampler.html

The sampler stores an array of `PerlinNoiseSampler` instances. Public
constructors accept a `ChunkRandom` plus an octave `IntStream`, `List<Integer>`,
or the mapped internal form. The public API also includes:

- `sample(double x, double y, double z)`
- `sample(double x, double y, double z, double, double, boolean)`
- `getOctave(int)`
- `maintainPrecision(double)`

The mapped static factory `method_30847(ChunkRandom, int, DoubleList)` is
present. The documentation does not expose how the octave list is expanded,
how absent octave slots are handled, the exact amplitude normalization, or
the constructor's full RNG consumption order.

### `DoublePerlinNoiseSampler`

Reference:
https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/util/math/noise/DoublePerlinNoiseSampler.html

The class contains an amplitude value and two
`OctavePerlinNoiseSampler` instances (`firstSampler` and `secondSampler`).
Its private constructor accepts `ChunkRandom`, an integer, and
`DoubleList`; its public sample method accepts three coordinates. The
Javadocs also list `createAmplitude(int)` and mapped factory
`method_30846(ChunkRandom, int, DoubleList)`.

The Javadocs do not document the precise combination formula or complete
random draw order for constructing both octave samplers.

### `MultiNoiseBiomeSource`

Reference:
https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/world/biome/source/MultiNoiseBiomeSource.html

The 1.16.5 class has four separate `DoublePerlinNoiseSampler` fields:
temperature, humidity, altitude, and weirdness. It also stores corresponding
noise-parameter fields, a seed, and biome points. The public biome lookup
entry point is `getBiomeForNoiseGen(int biomeX, int biomeY, int biomeZ)`.

This confirms that the biome query has four sampled noise channels. It does
not by itself establish the exact seed-to-sampler initialization or the
biome-point selection arithmetic.

### `ChunkRandom`

Reference:
https://maven.fabricmc.net/docs/yarn-1.16.5+build.5/net/minecraft/world/gen/ChunkRandom.html

The class extends `java.util.Random`. The Javadocs document constructors
with and without a seed, `consume(int)`, and seed helpers for terrain,
population, decorators, carvers, regions, and slime chunks. The documented
`consume` operation skips the requested number of randomizer calls.

The class documentation is useful for identifying the version's RNG
abstraction, but it does not provide the exact sampler-construction call
sequence needed for a seed-compatible Nether biome implementation.

## Implementation status and next parity requirements

The Rust port currently has a Java-compatible 48-bit random primitive and an
initial legacy improved-noise sampler. Those primitives are not yet proof of
full Minecraft 1.16.5 noise parity.

Before wiring seed-to-biome generation, the implementation still needs
version-specific confirmation for:

1. The exact Perlin constructor draw sequence and permutation initialization.
2. The octave index range, missing-octave behavior, frequency progression,
   amplitude normalization, and precision wrapping.
3. The DoublePerlin combination formula and the RNG state passed between its
   two octave samplers.
4. The four Nether noise parameter definitions and the precise seed used to
   initialize each sampler.
5. Numeric fixtures from an authoritative 1.16.5 runtime, covering sampler
   construction, sampled channel values, and biome selection at fixed seeds
   and coordinates.

Do not treat later-version documentation as proof of 1.16.5 behavior. Do not
copy or translate third-party implementation source; use public API
documentation and independently generated parity fixtures.
