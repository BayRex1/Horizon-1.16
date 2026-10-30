//! Wiring for the 1.16.5 Nether multi-noise biome lookup.
//!
//! The source accepts its four configured noise samplers from the caller. This
//! connects noise sampling to the legacy preset selector without pretending
//! that the sampler constructors are seed-compatible with vanilla yet.

use crate::legacy_1_16_5::{
    vanilla_nether_biome_from_noise, LegacyBiome, LegacyNetherNoiseSample,
};
use crate::legacy_double_perlin_noise_1_16_5::LegacyDoublePerlinNoise1_16_5;

/// Four-channel Nether biome source for the 1.16.5 climate preset.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyNetherBiomeSource1_16_5 {
    temperature: LegacyDoublePerlinNoise1_16_5,
    humidity: LegacyDoublePerlinNoise1_16_5,
    altitude: LegacyDoublePerlinNoise1_16_5,
    weirdness: LegacyDoublePerlinNoise1_16_5,
}

impl LegacyNetherBiomeSource1_16_5 {
    /// Builds a source from four already-configured noise samplers.
    ///
    /// The caller is responsible for supplying the correct vanilla 1.16.5
    /// sampler parameters and seed stream. This constructor does not infer
    /// those version-specific details.
    pub fn new(
        temperature: LegacyDoublePerlinNoise1_16_5,
        humidity: LegacyDoublePerlinNoise1_16_5,
        altitude: LegacyDoublePerlinNoise1_16_5,
        weirdness: LegacyDoublePerlinNoise1_16_5,
    ) -> Self {
        Self {
            temperature,
            humidity,
            altitude,
            weirdness,
        }
    }

    /// Samples the four climate channels at block-space coordinates.
    pub fn sample(&self, x: f64, y: f64, z: f64) -> LegacyNetherNoiseSample {
        LegacyNetherNoiseSample {
            temperature: self.temperature.sample(x, y, z) as f32,
            humidity: self.humidity.sample(x, y, z) as f32,
            altitude: self.altitude.sample(x, y, z) as f32,
            weirdness: self.weirdness.sample(x, y, z) as f32,
        }
    }

    /// Samples climate noise and resolves the nearest built-in Nether biome.
    ///
    /// Non-finite coordinates or noise results resolve to None. The nearest
    /// point selection remains in the version-pinned legacy biome adapter.
    pub fn biome_at(
        &self,
        x: f64,
        y: f64,
        z: f64,
    ) -> Result<Option<LegacyBiome>, serde_json::Error> {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Ok(None);
        }
        vanilla_nether_biome_from_noise(self.sample(x, y, z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy_octave_noise_1_16_5::LegacyOctaveStack1_16_5;

    fn zero_noise() -> LegacyDoublePerlinNoise1_16_5 {
        LegacyDoublePerlinNoise1_16_5::new(
            LegacyOctaveStack1_16_5::new(Vec::new()),
            LegacyOctaveStack1_16_5::new(Vec::new()),
            1.0,
            1.0,
        )
        .expect("finite composition parameters")
    }

    #[test]
    fn samples_all_four_channels_and_resolves_default_climate() {
        let source = LegacyNetherBiomeSource1_16_5::new(
            zero_noise(),
            zero_noise(),
            zero_noise(),
            zero_noise(),
        );
        assert_eq!(
            source.sample(12.0, 64.0, -8.0),
            LegacyNetherNoiseSample {
                temperature: 0.0,
                humidity: 0.0,
                altitude: 0.0,
                weirdness: 0.0,
            }
        );
        assert_eq!(
            source
                .biome_at(12.0, 64.0, -8.0)
                .expect("preset data should parse")
                .expect("zero climate resolves"),
            crate::legacy_1_16_5::biome_by_name("nether_wastes")
                .expect("biome data should parse")
                .expect("Nether wastes exists")
        );
    }

    #[test]
    fn rejects_non_finite_sample_coordinates() {
        let source = LegacyNetherBiomeSource1_16_5::new(
            zero_noise(),
            zero_noise(),
            zero_noise(),
            zero_noise(),
        );
        assert!(source.biome_at(f64::NAN, 0.0, 0.0).unwrap().is_none());
        assert!(source.biome_at(0.0, f64::INFINITY, 0.0).unwrap().is_none());
    }
}
