//! Explicit two-stack DoublePerlin composition for legacy 1.16.5 work.
//!
//! The public 1.16.5 API exposes two octave samplers and an amplitude, but its
//! Javadocs do not specify constructor RNG consumption or the domain transform.
//! This type therefore requires those values explicitly; it is not yet a
//! seed-compatible replacement for vanilla DoublePerlinNoiseSampler.

use crate::legacy_octave_noise_1_16_5::LegacyOctaveStack1_16_5;

/// A two-field Perlin composition with an explicit second-field domain scale.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyDoublePerlinNoise1_16_5 {
    first: LegacyOctaveStack1_16_5,
    second: LegacyOctaveStack1_16_5,
    second_domain_scale: f64,
    amplitude: f64,
}

impl LegacyDoublePerlinNoise1_16_5 {
    /// Creates a composition when all scalar parameters are finite.
    ///
    /// The caller supplies the domain scale and normalization amplitude so
    /// version-specific constants are never silently guessed.
    pub fn new(
        first: LegacyOctaveStack1_16_5,
        second: LegacyOctaveStack1_16_5,
        second_domain_scale: f64,
        amplitude: f64,
    ) -> Option<Self> {
        if !second_domain_scale.is_finite() || !amplitude.is_finite() {
            return None;
        }
        Some(Self {
            first,
            second,
            second_domain_scale,
            amplitude,
        })
    }

    pub fn first(&self) -> &LegacyOctaveStack1_16_5 {
        &self.first
    }

    pub fn second(&self) -> &LegacyOctaveStack1_16_5 {
        &self.second
    }

    pub fn second_domain_scale(&self) -> f64 {
        self.second_domain_scale
    }

    pub fn amplitude(&self) -> f64 {
        self.amplitude
    }

    /// Samples both fields and applies the caller-provided normalization.
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let first = self.first.sample(x, y, z);
        let scale = self.second_domain_scale;
        let second = self.second.sample(x * scale, y * scale, z * scale);
        (first + second) * self.amplitude
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy_improved_noise_1_16_5::LegacyImprovedNoise1_16_5;
    use crate::legacy_java_random_1_16_5::JavaRandom48;
    use crate::legacy_octave_noise_1_16_5::LegacyNoiseOctave1_16_5;

    fn stack(seed: i64, frequency: f64, amplitude: f64) -> LegacyOctaveStack1_16_5 {
        let sampler = LegacyImprovedNoise1_16_5::new(&mut JavaRandom48::new(seed));
        let octave = LegacyNoiseOctave1_16_5::new(sampler, frequency, amplitude).unwrap();
        LegacyOctaveStack1_16_5::new(vec![octave])
    }

    #[test]
    fn rejects_non_finite_composition_parameters() {
        assert!(LegacyDoublePerlinNoise1_16_5::new(
            stack(1, 1.0, 1.0),
            stack(2, 1.0, 1.0),
            f64::NAN,
            1.0,
        )
        .is_none());
        assert!(LegacyDoublePerlinNoise1_16_5::new(
            stack(1, 1.0, 1.0),
            stack(2, 1.0, 1.0),
            1.0,
            f64::INFINITY,
        )
        .is_none());
    }

    #[test]
    fn combines_both_fields_with_explicit_scale_and_amplitude() {
        let first = stack(17, 1.0, 1.0);
        let second = stack(23, 1.0, 1.0);
        let point = [2.5, -4.0, 9.25];
        let scale = 1.75;
        let amplitude = 0.125;
        let expected = (first.sample(point[0], point[1], point[2])
            + second.sample(point[0] * scale, point[1] * scale, point[2] * scale))
            * amplitude;
        let sampler = LegacyDoublePerlinNoise1_16_5::new(
            first,
            second,
            scale,
            amplitude,
        )
        .unwrap();
        assert_eq!(sampler.sample(point[0], point[1], point[2]), expected);
    }

    #[test]
    fn zero_amplitude_produces_zero() {
        let sampler = LegacyDoublePerlinNoise1_16_5::new(
            stack(31, 1.0, 1.0),
            stack(37, 1.0, 1.0),
            1.0,
            0.0,
        )
        .unwrap();
        assert_eq!(sampler.sample(3.0, 5.0, 7.0), 0.0);
    }
}
