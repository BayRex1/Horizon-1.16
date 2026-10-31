//! Seeded and explicitly configured Nether biome sources for Minecraft 1.16.5.
//!
//! The seeded constructor uses the version-pinned legacy Java Random /
//! DoublePerlin pipeline. The explicit constructor remains available for
//! callers that need to inject synthetic climate fields in tests.

use crate::legacy_1_16_5::{LegacyBiome, LegacyNetherNoiseSample, vanilla_nether_biome_from_noise};
use crate::legacy_double_perlin_noise_1_16_5::LegacyDoublePerlinNoise1_16_5;
use crate::legacy_seeded_noise_1_16_5::LegacyNetherNoise1_16_5;

#[derive(Clone, Debug, PartialEq)]
enum NetherNoiseSource1_16_5 {
    Explicit {
        temperature: LegacyDoublePerlinNoise1_16_5,
        humidity: LegacyDoublePerlinNoise1_16_5,
        altitude: LegacyDoublePerlinNoise1_16_5,
        weirdness: LegacyDoublePerlinNoise1_16_5,
    },
    Seeded(LegacyNetherNoise1_16_5),
}

/// Nether multi-noise biome source for the Minecraft Java 1.16.5 preset.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyNetherBiomeSource1_16_5 {
    source: NetherNoiseSource1_16_5,
}

impl LegacyNetherBiomeSource1_16_5 {
    /// Builds a source from four explicitly configured climate samplers.
    ///
    /// This is useful for controlled tests and custom climate configurations.
    /// For vanilla seed-derived Nether climate, use [Self::from_seed].
    pub fn new(
        temperature: LegacyDoublePerlinNoise1_16_5,
        humidity: LegacyDoublePerlinNoise1_16_5,
        altitude: LegacyDoublePerlinNoise1_16_5,
        weirdness: LegacyDoublePerlinNoise1_16_5,
    ) -> Self {
        Self {
            source: NetherNoiseSource1_16_5::Explicit {
                temperature,
                humidity,
                altitude,
                weirdness,
            },
        }
    }

    /// Constructs the vanilla 1.16.5 Nether climate source from the world seed.
    ///
    /// Temperature is seeded from the world seed and humidity from
    /// world_seed + 1. The built-in Nether preset fixes altitude and weirdness
    /// to zero; they are not additional random noise channels.
    pub fn from_seed(world_seed: i64) -> Result<Self, &'static str> {
        Ok(Self {
            source: NetherNoiseSource1_16_5::Seeded(LegacyNetherNoise1_16_5::from_seed(
                world_seed,
            )?),
        })
    }

    /// Samples the climate fields.
    ///
    /// For an explicitly configured source, coordinates retain the historical
    /// block-space contract. For a seed-derived Nether source, x/z are biome
    /// (quart-grid) coordinates and y is ignored, as in the 1.16.5 Nether
    /// biome source.
    pub fn sample(&self, x: f64, y: f64, z: f64) -> LegacyNetherNoiseSample {
        match &self.source {
            NetherNoiseSource1_16_5::Explicit {
                temperature,
                humidity,
                altitude,
                weirdness,
            } => LegacyNetherNoiseSample {
                temperature: temperature.sample(x, y, z) as f32,
                humidity: humidity.sample(x, y, z) as f32,
                altitude: altitude.sample(x, y, z) as f32,
                weirdness: weirdness.sample(x, y, z) as f32,
            },
            NetherNoiseSource1_16_5::Seeded(noise) => LegacyNetherNoiseSample {
                temperature: noise.temperature().sample(x, 0.0, z) as f32,
                humidity: noise.humidity().sample(x, 0.0, z) as f32,
                altitude: 0.0,
                weirdness: 0.0,
            },
        }
    }

    /// Samples at integer biome/quart coordinates using the seed-derived
    /// Nether source. Returns None for explicitly configured sources.
    pub fn sample_quart(&self, biome_x: i32, biome_z: i32) -> Option<LegacyNetherNoiseSample> {
        match &self.source {
            NetherNoiseSource1_16_5::Seeded(noise) => Some(noise.sample_quart(biome_x, biome_z)),
            NetherNoiseSource1_16_5::Explicit { .. } => None,
        }
    }

    /// Resolves the biome at integer biome/quart coordinates.
    pub fn biome_at_quart(
        &self,
        biome_x: i32,
        biome_z: i32,
    ) -> Result<Option<LegacyBiome>, serde_json::Error> {
        let Some(sample) = self.sample_quart(biome_x, biome_z) else {
            return Ok(None);
        };
        vanilla_nether_biome_from_noise(sample)
    }

    /// Resolves the legacy 1.16.5 numeric biome registry ID at quart coordinates.
    ///
    /// The returned ID belongs to the 1.16.5 biome registry and must not be
    /// interpreted as a modern Horizon biome ID.
    pub fn biome_id_at_quart(
        &self,
        biome_x: i32,
        biome_z: i32,
    ) -> Result<Option<u32>, serde_json::Error> {
        Ok(self.biome_at_quart(biome_x, biome_z)?.map(|biome| biome.id))
    }

    /// Samples climate noise and resolves the nearest built-in Nether biome.
    ///
    /// Non-finite coordinates or noise results resolve to None. For a seeded
    /// source, x/z are biome-grid coordinates; for an explicit source they
    /// retain the legacy block-space semantics.
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
    fn samples_all_explicit_channels_and_resolves_default_climate() {
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
    fn seeded_source_is_connected_to_real_seeded_climate_fields() {
        let source = LegacyNetherBiomeSource1_16_5::from_seed(987654321)
            .expect("built-in Nether sampler parameters are valid");
        let sample = source
            .sample_quart(12, -8)
            .expect("seeded source exposes quart sampling");
        assert_eq!(
            sample,
            crate::legacy_seeded_noise_1_16_5::LegacyNetherNoise1_16_5::from_seed(987654321)
                .unwrap()
                .sample_quart(12, -8)
        );
        assert_eq!(sample.altitude, 0.0);
        assert_eq!(sample.weirdness, 0.0);
        assert!(source.biome_at_quart(12, -8).unwrap().is_some());
    }

    #[test]
    fn quart_biome_id_is_from_the_legacy_registry() {
        let source = LegacyNetherBiomeSource1_16_5::from_seed(987654321).unwrap();
        let id = source.biome_id_at_quart(12, -8).unwrap().unwrap();
        let biome = crate::legacy_1_16_5::biome_by_id(id)
            .unwrap()
            .expect("returned ID exists in the 1.16.5 biome registry");
        assert!(matches!(
            biome.name.as_str(),
            "nether_wastes"
                | "soul_sand_valley"
                | "crimson_forest"
                | "warped_forest"
                | "basalt_deltas"
        ));
        assert_eq!(source.biome_at_quart(12, -8).unwrap().unwrap().id, id);
    }

    #[test]
    fn seeded_source_is_deterministic_for_a_world_seed() {
        let first = LegacyNetherBiomeSource1_16_5::from_seed(-123456789).unwrap();
        let second = LegacyNetherBiomeSource1_16_5::from_seed(-123456789).unwrap();
        for (x, z) in [(-128, 64), (0, 0), (31, -47), (2048, -1024)] {
            assert_eq!(first.sample_quart(x, z), second.sample_quart(x, z));
            assert_eq!(
                first.biome_at_quart(x, z).unwrap(),
                second.biome_at_quart(x, z).unwrap()
            );
        }
    }

    #[test]
    fn explicit_source_does_not_claim_seeded_quart_support() {
        let source = LegacyNetherBiomeSource1_16_5::new(
            zero_noise(),
            zero_noise(),
            zero_noise(),
            zero_noise(),
        );
        assert!(source.sample_quart(0, 0).is_none());
        assert!(source.biome_at_quart(0, 0).unwrap().is_none());
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
