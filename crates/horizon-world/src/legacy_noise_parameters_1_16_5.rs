//! Version-pinned data model for Minecraft Java 1.16.5 noise parameters.
//!
//! Yarn 1.16.5 exposes NoiseParameters as a first octave plus an ordered list
//! of amplitudes. This type preserves that input exactly; it does not infer
//! octave expansion, random-stream consumption, or sampler normalization.

/// The documented parameter pair used to configure legacy octave noise.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyNoiseParameters1_16_5 {
    first_octave: i32,
    amplitudes: Vec<f64>,
}

impl LegacyNoiseParameters1_16_5 {
    /// Creates parameters when every supplied amplitude is finite.
    ///
    /// Zero amplitudes are retained: their positions are meaningful because
    /// the list is ordered relative to `first_octave`.
    pub fn new(first_octave: i32, amplitudes: Vec<f64>) -> Option<Self> {
        if amplitudes.iter().any(|amplitude| !amplitude.is_finite()) {
            return None;
        }
        // Ensure the last represented octave can be addressed without overflow.
        if !amplitudes.is_empty()
            && first_octave
                .checked_add((amplitudes.len() - 1) as i32)
                .is_none()
        {
            return None;
        }
        Some(Self {
            first_octave,
            amplitudes,
        })
    }

    pub fn first_octave(&self) -> i32 {
        self.first_octave
    }

    pub fn amplitudes(&self) -> &[f64] {
        &self.amplitudes
    }

    pub fn is_empty(&self) -> bool {
        self.amplitudes.is_empty()
    }

    /// Returns the configured amplitude for an absolute octave index.
    pub fn amplitude_for_octave(&self, octave: i32) -> Option<f64> {
        let offset = octave.checked_sub(self.first_octave)?;
        if offset < 0 {
            return None;
        }
        self.amplitudes.get(offset as usize).copied()
    }

    /// Iterates configured (absolute octave, amplitude) pairs in source order.
    pub fn octave_amplitudes(&self) -> impl Iterator<Item = (i32, f64)> + '_ {
        self.amplitudes
            .iter()
            .copied()
            .enumerate()
            .map(move |(offset, amplitude)| (self.first_octave + offset as i32, amplitude))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_ordered_amplitudes_and_zero_slots() {
        let parameters = LegacyNoiseParameters1_16_5::new(-2, vec![1.0, 0.0, 0.5]).unwrap();
        assert_eq!(
            parameters.octave_amplitudes().collect::<Vec<_>>(),
            vec![(-2, 1.0), (-1, 0.0), (0, 0.5)]
        );
        assert_eq!(parameters.amplitude_for_octave(-1), Some(0.0));
        assert_eq!(parameters.amplitude_for_octave(0), Some(0.5));
        assert_eq!(parameters.amplitude_for_octave(-3), None);
        assert_eq!(parameters.amplitude_for_octave(1), None);
    }

    #[test]
    fn rejects_non_finite_amplitudes() {
        assert!(LegacyNoiseParameters1_16_5::new(0, vec![f64::NAN]).is_none());
        assert!(LegacyNoiseParameters1_16_5::new(0, vec![f64::INFINITY]).is_none());
    }

    #[test]
    fn rejects_octave_index_overflow() {
        assert!(LegacyNoiseParameters1_16_5::new(i32::MAX, vec![1.0, 1.0]).is_none());
        assert!(LegacyNoiseParameters1_16_5::new(i32::MIN, vec![1.0, 1.0]).is_some());
    }

    #[test]
    fn supports_empty_parameter_lists() {
        let parameters = LegacyNoiseParameters1_16_5::new(-8, Vec::new()).unwrap();
        assert!(parameters.is_empty());
        assert_eq!(parameters.octave_amplitudes().count(), 0);
    }
}
