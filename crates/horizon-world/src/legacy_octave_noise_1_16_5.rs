//! Explicit octave stacks built from legacy 1.16.5 improved-noise samplers.
//!
//! This module separates the sampling sum from vanilla's version-specific
//! octave constructor. Callers provide octave frequency and amplitude values
//! explicitly until constructor/RNG parity fixtures are available.

use crate::legacy_improved_noise_1_16_5::LegacyImprovedNoise1_16_5;

/// Period used by the 1.16.5 octave sampler to keep coordinates precise.
pub const LEGACY_NOISE_COORDINATE_PERIOD_1_16_5: f64 = 33_554_432.0;

/// Wraps a coordinate into the octave sampler's precision interval.
///
/// The 1.16.5 API exposes this operation as
/// `OctavePerlinNoiseSampler.maintainPrecision(double)`. Keeping it separate
/// lets parity tests pin the arithmetic before it is applied to seeded octave
/// construction.
pub fn maintain_precision_1_16_5(value: f64) -> f64 {
    let period = LEGACY_NOISE_COORDINATE_PERIOD_1_16_5;
    value - (value / period + 0.5).floor() * period
}

/// One independently configured octave in a noise stack.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyNoiseOctave1_16_5 {
    sampler: LegacyImprovedNoise1_16_5,
    frequency: f64,
    amplitude: f64,
}

impl LegacyNoiseOctave1_16_5 {
    /// Creates a layer with explicit coordinate frequency and output amplitude.
    pub fn new(sampler: LegacyImprovedNoise1_16_5, frequency: f64, amplitude: f64) -> Option<Self> {
        if !frequency.is_finite() || !amplitude.is_finite() {
            return None;
        }
        Some(Self {
            sampler,
            frequency,
            amplitude,
        })
    }

    pub fn frequency(&self) -> f64 {
        self.frequency
    }

    pub fn amplitude(&self) -> f64 {
        self.amplitude
    }

    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.sampler
            .sample(x * self.frequency, y * self.frequency, z * self.frequency)
            * self.amplitude
    }
}

/// A sum of explicitly configured legacy improved-noise octaves.
///
/// This is a reusable sampling primitive, not yet a drop-in implementation of
/// Minecraft's OctavePerlinNoiseSampler. Seed-based construction, octave-index
/// expansion, missing-octave random consumption and vanilla normalization
/// still require pinned 1.16.5 parity fixtures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacyOctaveStack1_16_5 {
    octaves: Vec<LegacyNoiseOctave1_16_5>,
}

impl LegacyOctaveStack1_16_5 {
    pub fn new(octaves: Vec<LegacyNoiseOctave1_16_5>) -> Self {
        Self { octaves }
    }

    pub fn octaves(&self) -> &[LegacyNoiseOctave1_16_5] {
        &self.octaves
    }

    pub fn is_empty(&self) -> bool {
        self.octaves.is_empty()
    }

    /// Adds the weighted contribution of every configured octave.
    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        self.octaves
            .iter()
            .map(|octave| octave.sample(x, y, z))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy_java_random_1_16_5::JavaRandom48;

    fn sampler(seed: i64) -> LegacyImprovedNoise1_16_5 {
        LegacyImprovedNoise1_16_5::new(&mut JavaRandom48::new(seed))
    }

    #[test]
    fn rejects_non_finite_layer_configuration() {
        assert!(LegacyNoiseOctave1_16_5::new(sampler(1), f64::NAN, 1.0).is_none());
        assert!(LegacyNoiseOctave1_16_5::new(sampler(1), 1.0, f64::INFINITY).is_none());
    }

    #[test]
    fn stack_sums_weighted_layers_and_respects_frequency() {
        let first = LegacyNoiseOctave1_16_5::new(sampler(7), 1.0, 1.0).unwrap();
        let second = LegacyNoiseOctave1_16_5::new(sampler(11), 2.0, 0.25).unwrap();
        let stack = LegacyOctaveStack1_16_5::new(vec![first.clone(), second.clone()]);
        let point = [3.25, -7.5, 12.0];
        let expected = first.sample(point[0], point[1], point[2])
            + second.sample(point[0], point[1], point[2]);
        assert_eq!(stack.sample(point[0], point[1], point[2]), expected);
    }

    #[test]
    fn empty_stack_is_zero() {
        assert_eq!(
            LegacyOctaveStack1_16_5::default().sample(1.0, 2.0, 3.0),
            0.0
        );
    }
}
