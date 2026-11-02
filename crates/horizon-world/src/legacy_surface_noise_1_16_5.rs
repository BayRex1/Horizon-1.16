//! Seeded Overworld terrain density noise for Minecraft Java 1.16.5.
//!
//! This is the legacy three-field density sampler (lower, upper and selector),
//! not a complete chunk generator. Its initialization order and sampling scales
//! follow the 1.16-era surface-noise pipeline documented by Yarn and the
//! independently maintained Cubiomes implementation.

use crate::legacy_improved_noise_1_16_5::LegacyImprovedNoise1_16_5;
use crate::legacy_java_random_1_16_5::JavaRandom48;
use crate::legacy_octave_noise_1_16_5::maintain_precision_1_16_5;

/// Vanilla horizontal and vertical base scales for 1.16.5 terrain density.
pub const LEGACY_TERRAIN_NOISE_SCALE_1_16_5: f64 = 684.412;
/// Horizontal coarse-cell divisor used by the selector noise.
pub const LEGACY_TERRAIN_HORIZONTAL_FACTOR_1_16_5: f64 = 80.0;
/// Vertical coarse-cell divisor used by the selector noise.
pub const LEGACY_TERRAIN_VERTICAL_FACTOR_1_16_5: f64 = 160.0;
/// Tiny scale correction present in the 1.16.x Overworld settings.
pub const LEGACY_TERRAIN_SCALE_CORRECTION_1_16_5: f64 = 0.999_999_981_450_774_5;

const LOWER_OCTAVES: usize = 16;
const UPPER_OCTAVES: usize = 16;
const SELECTOR_OCTAVES: usize = 8;

/// The three seeded Perlin stacks used to form 1.16.5 base terrain density.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacySurfaceNoise1_16_5 {
    lower: Vec<LegacyImprovedNoise1_16_5>,
    upper: Vec<LegacyImprovedNoise1_16_5>,
    selector: Vec<LegacyImprovedNoise1_16_5>,
}

impl LegacySurfaceNoise1_16_5 {
    /// Constructs the three fields from the world's Java Random seed.
    ///
    /// The order is significant: all lower octaves, then all upper octaves,
    /// then selector octaves, sharing one Java LCG stream.
    pub fn from_seed(seed: i64) -> Self {
        Self::from_random(&mut JavaRandom48::new(seed))
    }

    fn from_random(random: &mut JavaRandom48) -> Self {
        let lower = (0..LOWER_OCTAVES)
            .map(|_| LegacyImprovedNoise1_16_5::new(random))
            .collect();
        let upper = (0..UPPER_OCTAVES)
            .map(|_| LegacyImprovedNoise1_16_5::new(random))
            .collect();
        let selector = (0..SELECTOR_OCTAVES)
            .map(|_| LegacyImprovedNoise1_16_5::new(random))
            .collect();

        Self {
            lower,
            upper,
            selector,
        }
    }

    /// Samples the interpolated base density at a block coordinate.
    ///
    /// Positive values represent solid terrain; zero or negative values
    /// represent air. Surface replacement, caves, aquifers and biome shaping
    /// are separate generation stages and are intentionally not included.
    pub fn sample_density(&self, x: i32, y: i32, z: i32) -> f64 {
        let xz_scale = LEGACY_TERRAIN_NOISE_SCALE_1_16_5
            * LEGACY_TERRAIN_SCALE_CORRECTION_1_16_5;
        let y_scale = LEGACY_TERRAIN_NOISE_SCALE_1_16_5
            * LEGACY_TERRAIN_SCALE_CORRECTION_1_16_5;
        let xz_step = xz_scale / LEGACY_TERRAIN_HORIZONTAL_FACTOR_1_16_5;
        let y_step = y_scale / LEGACY_TERRAIN_VERTICAL_FACTOR_1_16_5;

        let mut lower = 0.0;
        let mut upper = 0.0;
        let mut selector = 0.0;
        let mut persistence = 1.0;
        let mut contribution = 1.0;

        for octave in 0..LOWER_OCTAVES {
            let px = maintain_precision_1_16_5(f64::from(x) * xz_scale * persistence);
            let py = maintain_precision_1_16_5(f64::from(y) * y_scale * persistence);
            let pz = maintain_precision_1_16_5(f64::from(z) * xz_scale * persistence);
            let y_amplitude = y_scale * persistence;
            let y_minimum = f64::from(y) * y_amplitude;

            lower += self.lower[octave].sample_with_y_scale(
                px, py, pz, y_amplitude, y_minimum,
            ) * contribution;
            upper += self.upper[octave].sample_with_y_scale(
                px, py, pz, y_amplitude, y_minimum,
            ) * contribution;

            if octave < SELECTOR_OCTAVES {
                let sx = maintain_precision_1_16_5(f64::from(x) * xz_step * persistence);
                let sy = maintain_precision_1_16_5(f64::from(y) * y_step * persistence);
                let sz = maintain_precision_1_16_5(f64::from(z) * xz_step * persistence);
                let selector_y_amplitude = y_step * persistence;
                let selector_y_minimum = f64::from(y) * selector_y_amplitude;

                selector += self.selector[octave].sample_with_y_scale(
                    sx,
                    sy,
                    sz,
                    selector_y_amplitude,
                    selector_y_minimum,
                ) * contribution;
            }

            persistence *= 0.5;
            contribution *= 2.0;
        }

        let blend = (0.5 + 0.05 * selector).clamp(0.0, 1.0);
        let lower = lower / 512.0;
        let upper = upper / 512.0;
        lower + blend * (upper - lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_consumes_legacy_fields_in_vanilla_order() {
        let seed = -123_456_789;
        let mut actual_random = JavaRandom48::new(seed);
        let _noise = LegacySurfaceNoise1_16_5::from_random(&mut actual_random);

        let mut expected_random = JavaRandom48::new(seed);
        for _ in 0..(LOWER_OCTAVES + UPPER_OCTAVES + SELECTOR_OCTAVES) {
            let _ = LegacyImprovedNoise1_16_5::new(&mut expected_random);
        }

        assert_eq!(actual_random, expected_random);
    }

    #[test]
    fn seeded_density_is_deterministic_and_finite() {
        let first = LegacySurfaceNoise1_16_5::from_seed(987_654_321);
        let second = LegacySurfaceNoise1_16_5::from_seed(987_654_321);

        for point in [[0, 64, 0], [128, 72, -256], [-4096, 32, 8192]] {
            let a = first.sample_density(point[0], point[1], point[2]);
            let b = second.sample_density(point[0], point[1], point[2]);
            assert!(a.is_finite());
            assert_eq!(a, b);
        }
    }

    #[test]
    fn different_seeds_change_the_density_field() {
        let first = LegacySurfaceNoise1_16_5::from_seed(1);
        let second = LegacySurfaceNoise1_16_5::from_seed(2);

        assert_ne!(
            first.sample_density(120, 64, -75),
            second.sample_density(120, 64, -75)
        );
    }
}
