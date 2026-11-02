//! Trilinear interpolation primitives for the legacy 1.16.5 terrain sampler.
//!
//! Vanilla 1.16.x evaluates lower, upper and selector noise at coarse cell
//! corners, then interpolates density inside each cell. This module contains
//! only the interpolation step; it does not implement the complete vanilla
//! density formula or noise initialization.

/// Horizontal size of one legacy Overworld noise cell, in blocks.
pub const LEGACY_NOISE_CELL_WIDTH_1_16_5: u8 = 4;
/// Vertical size of one legacy Overworld noise cell, in blocks.
pub const LEGACY_NOISE_CELL_HEIGHT_1_16_5: u8 = 8;

/// Interpolates the eight corners of one noise cell.
///
/// Corner order: (x0,y0,z0), (x1,y0,z0), (x0,y1,z0), (x1,y1,z0),
/// (x0,y0,z1), (x1,y0,z1), (x0,y1,z1), (x1,y1,z1).
/// Fractions are clamped to the cell to prevent accidental extrapolation.
pub fn trilinear_interpolate_1_16_5(corners: [f64; 8], fx: f64, fy: f64, fz: f64) -> f64 {
    let fx = fx.clamp(0.0, 1.0);
    let fy = fy.clamp(0.0, 1.0);
    let fz = fz.clamp(0.0, 1.0);
    let x00 = lerp(corners[0], corners[1], fx);
    let x10 = lerp(corners[2], corners[3], fx);
    let x01 = lerp(corners[4], corners[5], fx);
    let x11 = lerp(corners[6], corners[7], fx);
    let y0 = lerp(x00, x10, fy);
    let y1 = lerp(x01, x11, fy);
    lerp(y0, y1, fz)
}

/// One 4×8×4 block region represented by its eight sampled noise corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyNoiseCell1_16_5 {
    corners: [f64; 8],
}

impl LegacyNoiseCell1_16_5 {
    pub const fn new(corners: [f64; 8]) -> Self {
        Self { corners }
    }

    /// Interpolates density at a block offset inside this cell.
    ///
    /// Offsets follow vanilla's sampling order: x and z are 0..4, y is 0..8.
    /// The upper boundary belongs to the neighbouring cell and is rejected.
    pub fn sample_block(&self, x: u8, y: u8, z: u8) -> Option<f64> {
        if x >= LEGACY_NOISE_CELL_WIDTH_1_16_5
            || y >= LEGACY_NOISE_CELL_HEIGHT_1_16_5
            || z >= LEGACY_NOISE_CELL_WIDTH_1_16_5
        {
            return None;
        }

        Some(trilinear_interpolate_1_16_5(
            self.corners,
            f64::from(x) / f64::from(LEGACY_NOISE_CELL_WIDTH_1_16_5),
            f64::from(y) / f64::from(LEGACY_NOISE_CELL_HEIGHT_1_16_5),
            f64::from(z) / f64::from(LEGACY_NOISE_CELL_WIDTH_1_16_5),
        ))
    }

    pub const fn corners(&self) -> &[f64; 8] {
        &self.corners
    }
}

/// Blends lower and upper density limits with the vanilla selector convention.
pub fn blend_density_limits_1_16_5(lower: f64, upper: f64, selector: f64) -> f64 {
    let weight = ((selector / 10.0 + 1.0) / 2.0).clamp(0.0, 1.0);
    lerp(lower, upper, weight)
}

#[inline]
fn lerp(start: f64, end: f64, amount: f64) -> f64 {
    start + amount * (end - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trilinear_interpolation_returns_cell_corners() {
        let c = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        assert_eq!(trilinear_interpolate_1_16_5(c, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(trilinear_interpolate_1_16_5(c, 1.0, 0.0, 0.0), 1.0);
        assert_eq!(trilinear_interpolate_1_16_5(c, 0.0, 1.0, 0.0), 2.0);
        assert_eq!(trilinear_interpolate_1_16_5(c, 1.0, 1.0, 1.0), 7.0);
    }

    #[test]
    fn trilinear_interpolation_averages_cell_center() {
        let c = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        assert_eq!(trilinear_interpolate_1_16_5(c, 0.5, 0.5, 0.5), 3.5);
    }

    #[test]
    fn trilinear_interpolation_clamps_fractions() {
        let c = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        assert_eq!(trilinear_interpolate_1_16_5(c, -2.0, 0.0, 0.0), 0.0);
        assert_eq!(trilinear_interpolate_1_16_5(c, 2.0, 1.0, 1.0), 7.0);
    }

    #[test]
    fn block_sampling_uses_legacy_four_by_eight_by_four_cell_geometry() {
        let cell = LegacyNoiseCell1_16_5::new([0.0, 4.0, 8.0, 12.0, 16.0, 20.0, 24.0, 28.0]);
        assert_eq!(cell.sample_block(0, 0, 0), Some(0.0));
        assert_eq!(cell.sample_block(2, 4, 2), Some(14.0));
        assert_eq!(cell.sample_block(3, 7, 3), Some(24.5));
    }

    #[test]
    fn block_sampling_rejects_offsets_outside_the_cell() {
        let cell = LegacyNoiseCell1_16_5::new([0.0; 8]);
        assert_eq!(cell.sample_block(4, 0, 0), None);
        assert_eq!(cell.sample_block(0, 8, 0), None);
        assert_eq!(cell.sample_block(0, 0, 4), None);
    }

    #[test]
    fn selector_blends_lower_middle_and_upper_limits() {
        assert_eq!(blend_density_limits_1_16_5(2.0, 6.0, -10.0), 2.0);
        assert_eq!(blend_density_limits_1_16_5(2.0, 6.0, 0.0), 4.0);
        assert_eq!(blend_density_limits_1_16_5(2.0, 6.0, 10.0), 6.0);
        assert_eq!(blend_density_limits_1_16_5(2.0, 6.0, 100.0), 6.0);
    }
}
