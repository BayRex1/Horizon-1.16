//! Seeded legacy Perlin and DoublePerlin noise for Minecraft Java 1.16.5.
//!
//! The implementation follows the legacy Java Random stream and the
//! 1.16-era octave layout used by Nether climate noise. It intentionally does
//! not use the post-1.18 positional/Xoroshiro noise pipeline.
//!
//! Reference checks:
//! - Yarn 1.16.5 documents the ChunkRandom-backed octave and DoublePerlin APIs.
//! - Cubiomes' independently maintained 1.16+ Nether implementation pins the
//!   two-octave (-7, length 2) temperature/humidity samplers, Java LCG skips,
//!   337/331 second-domain scale, and 10/9 normalization.

use crate::legacy_1_16_5::{
    LegacyBiome, LegacyNetherNoiseSample, vanilla_nether_biome_from_noise,
};
use crate::legacy_improved_noise_1_16_5::LegacyImprovedNoise1_16_5;
use crate::legacy_java_random_1_16_5::JavaRandom48;
use crate::legacy_noise_parameters_1_16_5::LegacyNoiseParameters1_16_5;

/// Number of Java Random state transitions consumed by one absent legacy
/// Perlin octave in the 1.16.5 world-generation stream.
pub const LEGACY_PERLIN_OCTAVE_RANDOM_DRAWS_1_16_5: u64 = 262;

/// Domain multiplier applied to the second half of legacy DoublePerlin noise.
pub const LEGACY_DOUBLE_PERLIN_DOMAIN_SCALE_1_16_5: f64 = 337.0 / 331.0;

/// Seeded Improved/Perlin noise layer, including its original Java offsets.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyPerlinNoiseSampler1_16_5 {
    improved: LegacyImprovedNoise1_16_5,
}

impl LegacyPerlinNoiseSampler1_16_5 {
    pub fn new(random: &mut JavaRandom48) -> Self {
        Self {
            improved: LegacyImprovedNoise1_16_5::new(random),
        }
    }

    pub fn origin_y(&self) -> f64 {
        self.improved.offsets()[1]
    }

    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.sample_with_y_scale(x, y, z, 0.0, 0.0)
    }

    pub fn sample_with_y_scale(
        &self,
        x: f64,
        y: f64,
        z: f64,
        y_scale: f64,
        min_y: f64,
    ) -> f64 {
        self.improved
            .sample_with_y_scale(x, y, z, y_scale, min_y)
    }
}

/// Seeded octave sampler matching the legacy first-octave/amplitude model.
///
/// Layers are constructed from the highest represented octave down to the
/// lowest. Missing octaves above the configured range and zero-amplitude
/// slots consume the same fixed 262-step Java LCG skips as vanilla.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyOctavePerlinNoiseSampler1_16_5 {
    layers: Vec<Option<LegacyPerlinNoiseSampler1_16_5>>,
    amplitudes: Vec<f64>,
    frequencies: Vec<f64>,
}

impl LegacyOctavePerlinNoiseSampler1_16_5 {
    pub fn new(
        random: &mut JavaRandom48,
        parameters: &LegacyNoiseParameters1_16_5,
    ) -> Result<Self, &'static str> {
        let amplitudes = parameters.amplitudes();
        if amplitudes.is_empty() {
            return Err("legacy octave noise requires at least one amplitude");
        }
        if amplitudes.len() > 64 {
            return Err("legacy octave amplitude list exceeds the supported bound");
        }

        let last_octave = parameters
            .first_octave()
            .checked_add(amplitudes.len() as i32 - 1)
            .ok_or("legacy octave range overflows i32")?;

        // Vanilla first advances over every absent octave between octave 0 and
        // the highest configured octave. Each absent octave consumes 262
        // calls to Random.next(1), not 262 calls to nextInt(256).
        if last_octave < 0 {
            let absent = last_octave
                .checked_neg()
                .ok_or("legacy octave skip count overflows i32")? as u64;
            random.advance(
                absent
                    .checked_mul(LEGACY_PERLIN_OCTAVE_RANDOM_DRAWS_1_16_5)
                    .ok_or("legacy octave skip count overflows u64")?,
            );
        }

        let count = amplitudes.len();
        let mut layer_amplitudes = Vec::with_capacity(count);
        let mut frequencies = Vec::with_capacity(count);
        let mut layers = Vec::with_capacity(count);
        let mut amplitude_factor = 1.0 / (2.0_f64.powi(count as i32) - 1.0);
        let mut frequency = 2.0_f64.powi(last_octave);

        // The source parameter list is lowest-to-highest octave, while the
        // legacy constructor initializes its sampler array high-to-low.
        for amplitude in amplitudes.iter().rev().copied() {
            if amplitude == 0.0 {
                random.advance(LEGACY_PERLIN_OCTAVE_RANDOM_DRAWS_1_16_5);
                layers.push(None);
            } else {
                layers.push(Some(LegacyPerlinNoiseSampler1_16_5::new(random)));
            }
            layer_amplitudes.push(amplitude * amplitude_factor);
            frequencies.push(frequency);
            amplitude_factor *= 2.0;
            frequency *= 0.5;
        }

        Ok(Self {
            layers,
            amplitudes: layer_amplitudes,
            frequencies,
        })
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    /// Samples the complete stack at coordinates supplied in biome/noise space.
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.sample_with_y_control(x, y, z, 0.0, 0.0, false)
    }

    /// Full legacy sampler entry point, including vertical quantization and
    /// the optional use of each layer's own y-origin.
    pub fn sample_with_y_control(
        &self,
        x: f64,
        y: f64,
        z: f64,
        y_scale: f64,
        min_y: f64,
        use_default_y: bool,
    ) -> f64 {
        let mut value = 0.0;
        for index in 0..self.layers.len() {
            let Some(layer) = &self.layers[index] else {
                continue;
            };
            let frequency = self.frequencies[index];
            let sample_y = if use_default_y {
                -layer.origin_y()
            } else {
                maintain_precision_1_16_5(y * frequency)
            };
            let contribution = layer.sample_with_y_scale(
                maintain_precision_1_16_5(x * frequency),
                sample_y,
                maintain_precision_1_16_5(z * frequency),
                y_scale * frequency,
                min_y * frequency,
            );
            value += self.amplitudes[index] * contribution;
        }
        value
    }
}

/// Legacy DoublePerlin sampler used by 1.16.x Nether climate generation.
///
/// Both octave stacks consume one shared Java Random stream in sequence. The
/// second stack samples a slightly scaled coordinate domain (337/331).
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyDoublePerlinNoiseSampler1_16_5 {
    first: LegacyOctavePerlinNoiseSampler1_16_5,
    second: LegacyOctavePerlinNoiseSampler1_16_5,
    amplitude: f64,
}

impl LegacyDoublePerlinNoiseSampler1_16_5 {
    pub fn new(
        random: &mut JavaRandom48,
        parameters: &LegacyNoiseParameters1_16_5,
    ) -> Result<Self, &'static str> {
        let first = LegacyOctavePerlinNoiseSampler1_16_5::new(random, parameters)?;
        let second = LegacyOctavePerlinNoiseSampler1_16_5::new(random, parameters)?;

        let first_nonzero = parameters
            .amplitudes()
            .iter()
            .position(|amplitude| *amplitude != 0.0)
            .ok_or("legacy DoublePerlin requires a non-zero amplitude")?;
        let last_nonzero = parameters
            .amplitudes()
            .iter()
            .rposition(|amplitude| *amplitude != 0.0)
            .ok_or("legacy DoublePerlin requires a non-zero amplitude")?;
        let octave_span = (last_nonzero - first_nonzero) as f64;
        let expected_deviation = 0.1 * (1.0 + 1.0 / (octave_span + 1.0));
        let amplitude = (1.0 / 6.0) / expected_deviation;

        Ok(Self {
            first,
            second,
            amplitude,
        })
    }

    pub fn from_seed(
        seed: i64,
        parameters: &LegacyNoiseParameters1_16_5,
    ) -> Result<Self, &'static str> {
        Self::new(&mut JavaRandom48::new(seed), parameters)
    }

    pub fn amplitude(&self) -> f64 {
        self.amplitude
    }

    pub fn first(&self) -> &LegacyOctavePerlinNoiseSampler1_16_5 {
        &self.first
    }

    pub fn second(&self) -> &LegacyOctavePerlinNoiseSampler1_16_5 {
        &self.second
    }

    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        (self.first.sample(x, y, z)
            + self.second.sample(
                x * LEGACY_DOUBLE_PERLIN_DOMAIN_SCALE_1_16_5,
                y * LEGACY_DOUBLE_PERLIN_DOMAIN_SCALE_1_16_5,
                z * LEGACY_DOUBLE_PERLIN_DOMAIN_SCALE_1_16_5,
            ))
            * self.amplitude
    }
}

/// Seed-derived Minecraft 1.16.x Nether climate noise.
///
/// The Nether preset uses two independent DoublePerlin fields: temperature
/// from world seed N and humidity from N+1. Altitude and weirdness are fixed at
/// zero for the built-in 1.16.x Nether preset, so they are not separately
/// seeded noise channels.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyNetherNoise1_16_5 {
    temperature: LegacyDoublePerlinNoiseSampler1_16_5,
    humidity: LegacyDoublePerlinNoiseSampler1_16_5,
}

impl LegacyNetherNoise1_16_5 {
    pub fn from_seed(world_seed: i64) -> Result<Self, &'static str> {
        let parameters = LegacyNoiseParameters1_16_5::new(-7, vec![1.0, 1.0])
            .ok_or("invalid built-in Nether noise parameters")?;
        let temperature =
            LegacyDoublePerlinNoiseSampler1_16_5::from_seed(world_seed, &parameters)?;
        let humidity = LegacyDoublePerlinNoiseSampler1_16_5::from_seed(
            world_seed.wrapping_add(1),
            &parameters,
        )?;
        Ok(Self {
            temperature,
            humidity,
        })
    }

    pub fn temperature(&self) -> &LegacyDoublePerlinNoiseSampler1_16_5 {
        &self.temperature
    }

    pub fn humidity(&self) -> &LegacyDoublePerlinNoiseSampler1_16_5 {
        &self.humidity
    }

    /// Samples Nether biome coordinates (the 1:4 biome grid). The vanilla
    /// Nether climate lookup uses y=0; the caller must supply quart x/z.
    pub fn sample_quart(&self, biome_x: i32, biome_z: i32) -> LegacyNetherNoiseSample {
        LegacyNetherNoiseSample {
            temperature: self.temperature.sample(biome_x as f64, 0.0, biome_z as f64) as f32,
            humidity: self.humidity.sample(biome_x as f64, 0.0, biome_z as f64) as f32,
            altitude: 0.0,
            weirdness: 0.0,
        }
    }

    pub fn biome_at_quart(
        &self,
        biome_x: i32,
        biome_z: i32,
    ) -> Result<Option<LegacyBiome>, serde_json::Error> {
        vanilla_nether_biome_from_noise(self.sample_quart(biome_x, biome_z))
    }
}

/// Vanilla's 1.16.5 octave coordinate precision correction.
pub fn maintain_precision_1_16_5(value: f64) -> f64 {
    const PERIOD: f64 = 33_554_432.0;
    value - (value / PERIOD + 0.5).floor() * PERIOD
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nether_parameters() -> LegacyNoiseParameters1_16_5 {
        LegacyNoiseParameters1_16_5::new(-7, vec![1.0, 1.0]).unwrap()
    }

    #[test]
    fn legacy_nether_octaves_skip_the_six_absent_higher_octaves() {
        let mut actual = JavaRandom48::new(1234);
        let _sampler =
            LegacyOctavePerlinNoiseSampler1_16_5::new(&mut actual, &nether_parameters()).unwrap();

        let mut expected = JavaRandom48::new(1234);
        expected.advance(6 * LEGACY_PERLIN_OCTAVE_RANDOM_DRAWS_1_16_5);
        let _first = LegacyPerlinNoiseSampler1_16_5::new(&mut expected);
        let _second = LegacyPerlinNoiseSampler1_16_5::new(&mut expected);
        assert_eq!(actual, expected);
    }

    #[test]
    fn zero_amplitude_slot_consumes_exactly_one_missing_octave() {
        let parameters =
            LegacyNoiseParameters1_16_5::new(-2, vec![1.0, 0.0, 1.0]).unwrap();
        let mut actual = JavaRandom48::new(77);
        let sampler =
            LegacyOctavePerlinNoiseSampler1_16_5::new(&mut actual, &parameters).unwrap();
        assert_eq!(sampler.layer_count(), 3);

        let mut expected = JavaRandom48::new(77);
        let _highest = LegacyPerlinNoiseSampler1_16_5::new(&mut expected);
        expected.advance(LEGACY_PERLIN_OCTAVE_RANDOM_DRAWS_1_16_5);
        let _lowest = LegacyPerlinNoiseSampler1_16_5::new(&mut expected);
        assert_eq!(actual, expected);
    }

    #[test]
    fn two_adjacent_nonzero_octaves_use_the_vanilla_double_perlin_factor() {
        let sampler =
            LegacyDoublePerlinNoiseSampler1_16_5::from_seed(1, &nether_parameters()).unwrap();
        assert!((sampler.amplitude() - (10.0 / 9.0)).abs() < 1.0e-14);
    }

    #[test]
    fn seeded_nether_noise_is_deterministic_and_finite() {
        let first = LegacyNetherNoise1_16_5::from_seed(987654321).unwrap();
        let second = LegacyNetherNoise1_16_5::from_seed(987654321).unwrap();
        for (x, z) in [(-128, 64), (0, 0), (31, -47), (2048, -1024)] {
            assert_eq!(first.sample_quart(x, z), second.sample_quart(x, z));
            assert!(first.sample_quart(x, z).is_finite());
        }
    }

    #[test]
    fn legacy_nether_seed_fixture_is_stable_at_known_quart_coordinates() {
        let noise = LegacyNetherNoise1_16_5::from_seed(987654321).unwrap();
        let sample = noise.sample_quart(12, -8);
        assert!((sample.temperature - (-0.06475469)).abs() < 1.0e-6);
        assert!((sample.humidity - (-0.3163646)).abs() < 1.0e-6);
        assert_eq!(sample.altitude, 0.0);
        assert_eq!(sample.weirdness, 0.0);
    }

    #[test]
    fn nether_seed_channels_are_independent() {
        let first = LegacyNetherNoise1_16_5::from_seed(42).unwrap();
        let second = LegacyNetherNoise1_16_5::from_seed(43).unwrap();
        assert_ne!(first.sample_quart(12, -8), second.sample_quart(12, -8));
    }

    #[test]
    fn maintain_precision_is_periodic() {
        let value = 12345.625;
        let period = 33_554_432.0;
        assert_eq!(
            maintain_precision_1_16_5(value),
            maintain_precision_1_16_5(value + period * 3.0)
        );
    }
}
