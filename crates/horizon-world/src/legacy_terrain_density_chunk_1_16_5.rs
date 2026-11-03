//! Coarse-cell density volume for one Minecraft Java 1.16.5 Overworld chunk.
//!
//! This is an intermediate world-generation representation. It evaluates the
//! seeded lower/upper/selector density field on the legacy 4x8x4 lattice and
//! interpolates it to block resolution. Biome shaping, surface replacement,
//! caves, structures and block-state assignment are later stages.

use crate::legacy_terrain_interpolation_1_16_5::{
    LEGACY_NOISE_CELL_HEIGHT_1_16_5, LEGACY_NOISE_CELL_WIDTH_1_16_5,
};
use crate::{
    legacy_1_16_5::{LegacyBlockSection, LegacyBlockState},
    legacy_surface_noise_1_16_5::LegacySurfaceNoise1_16_5,
};

pub const LEGACY_CHUNK_WIDTH_1_16_5: usize = 16;
pub const LEGACY_CHUNK_HEIGHT_1_16_5: usize = 256;
pub const LEGACY_CHUNK_DENSITY_COUNT_1_16_5: usize =
    LEGACY_CHUNK_WIDTH_1_16_5 * LEGACY_CHUNK_HEIGHT_1_16_5 * LEGACY_CHUNK_WIDTH_1_16_5;

const CELLS_XZ: usize = LEGACY_CHUNK_WIDTH_1_16_5 / LEGACY_NOISE_CELL_WIDTH_1_16_5 as usize;
const CELLS_Y: usize = LEGACY_CHUNK_HEIGHT_1_16_5 / LEGACY_NOISE_CELL_HEIGHT_1_16_5 as usize;
const GRID_XZ: usize = CELLS_XZ + 1;
const GRID_Y: usize = CELLS_Y + 1;

/// A block-resolution density field for one 16x256x16 legacy chunk column.
///
/// Positive density is solid terrain and non-positive density is air in the
/// base terrain stage. Values are stored as f32 to keep the intermediate
/// chunk representation compact.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyTerrainDensityChunk1_16_5 {
    pub x: i32,
    pub z: i32,
    densities: Vec<f32>,
}

impl LegacyTerrainDensityChunk1_16_5 {
    /// Generates one density volume from the world's already-seeded noise.
    ///
    /// Only 5x33x5 coarse samples are evaluated; the 65,536 block positions
    /// are filled by trilinear interpolation.
    pub fn generate(noise: &LegacySurfaceNoise1_16_5, chunk_x: i32, chunk_z: i32) -> Self {
        let mut coarse = vec![0.0_f64; GRID_XZ * GRID_Y * GRID_XZ];
        let coarse_index = |x: usize, y: usize, z: usize| (y * GRID_XZ + z) * GRID_XZ + x;

        // The 1.16.5 density sampler is evaluated on the 4x8x4 noise-cell
        // lattice, not in block coordinates. Its internal 684.412 scaling
        // converts these lattice coordinates into the vanilla noise domain.
        for gy in 0..GRID_Y {
            let cell_y = gy as i32;
            for gz in 0..GRID_XZ {
                let cell_z = chunk_z
                    .wrapping_mul(CELLS_XZ as i32)
                    .wrapping_add(gz as i32);
                for gx in 0..GRID_XZ {
                    let cell_x = chunk_x
                        .wrapping_mul(CELLS_XZ as i32)
                        .wrapping_add(gx as i32);
                    coarse[coarse_index(gx, gy, gz)] = noise.sample_density(cell_x, cell_y, cell_z);
                }
            }
        }

        let mut densities = vec![0.0_f32; LEGACY_CHUNK_DENSITY_COUNT_1_16_5];
        for cy in 0..CELLS_Y {
            for cz in 0..CELLS_XZ {
                for cx in 0..CELLS_XZ {
                    let x0 = cx;
                    let x1 = cx + 1;
                    let y0 = cy;
                    let y1 = cy + 1;
                    let z0 = cz;
                    let z1 = cz + 1;
                    let corners = [
                        coarse[coarse_index(x0, y0, z0)],
                        coarse[coarse_index(x1, y0, z0)],
                        coarse[coarse_index(x0, y1, z0)],
                        coarse[coarse_index(x1, y1, z0)],
                        coarse[coarse_index(x0, y0, z1)],
                        coarse[coarse_index(x1, y0, z1)],
                        coarse[coarse_index(x0, y1, z1)],
                        coarse[coarse_index(x1, y1, z1)],
                    ];

                    let cell =
                        crate::legacy_terrain_interpolation_1_16_5::LegacyNoiseCell1_16_5::new(
                            corners,
                        );
                    for oy in 0..LEGACY_NOISE_CELL_HEIGHT_1_16_5 {
                        for oz in 0..LEGACY_NOISE_CELL_WIDTH_1_16_5 {
                            for ox in 0..LEGACY_NOISE_CELL_WIDTH_1_16_5 {
                                let density = cell
                                    .sample_block(ox, oy, oz)
                                    .expect("loop offsets are inside a legacy noise cell");
                                let bx = cx * LEGACY_NOISE_CELL_WIDTH_1_16_5 as usize + ox as usize;
                                let by =
                                    cy * LEGACY_NOISE_CELL_HEIGHT_1_16_5 as usize + oy as usize;
                                let bz = cz * LEGACY_NOISE_CELL_WIDTH_1_16_5 as usize + oz as usize;
                                densities[(by * LEGACY_CHUNK_WIDTH_1_16_5 + bz)
                                    * LEGACY_CHUNK_WIDTH_1_16_5
                                    + bx] = density as f32;
                            }
                        }
                    }
                }
            }
        }

        Self {
            x: chunk_x,
            z: chunk_z,
            densities,
        }
    }

    /// Returns the interpolated density at a local block coordinate.
    pub fn density(&self, x: usize, y: usize, z: usize) -> Option<f32> {
        if x >= LEGACY_CHUNK_WIDTH_1_16_5
            || y >= LEGACY_CHUNK_HEIGHT_1_16_5
            || z >= LEGACY_CHUNK_WIDTH_1_16_5
        {
            return None;
        }
        Some(self.densities[(y * LEGACY_CHUNK_WIDTH_1_16_5 + z) * LEGACY_CHUNK_WIDTH_1_16_5 + x])
    }

    /// Returns the complete density array in Y, Z, X order.
    pub fn densities(&self) -> &[f32] {
        &self.densities
    }

    /// Whether this base-density sample is on the solid side of the surface.
    pub fn is_solid(&self, x: usize, y: usize, z: usize) -> Option<bool> {
        self.density(x, y, z).map(|density| density > 0.0)
    }

    /// Materializes this density field into legacy 1.16.5 block sections.
    ///
    /// This is a deliberately basic first terrain stage: solid density becomes
    /// stone, the upper three solid blocks become dirt, the exposed top becomes
    /// grass, and water fills open space up to sea level. It is not the complete
    /// vanilla surface-rule, bedrock-randomization, cave-carving or biome stage.
    pub fn to_legacy_sections(&self, sea_level: i32) -> std::io::Result<Vec<LegacyBlockSection>> {
        let air = LegacyBlockState::new("air");
        let bedrock = LegacyBlockState::new("bedrock");
        let stone = LegacyBlockState::new("stone");
        let dirt = LegacyBlockState::new("dirt");
        let grass = LegacyBlockState::new("grass_block");
        let water = LegacyBlockState::new("water");
        let mut sections: Vec<_> = (0..LEGACY_CHUNK_HEIGHT_1_16_5 / 16)
            .map(|_| LegacyBlockSection::uniform(air.clone()))
            .collect();

        for z in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
            for x in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
                let top = (0..LEGACY_CHUNK_HEIGHT_1_16_5)
                    .rev()
                    .find(|&y| self.is_solid(x, y, z) == Some(true))
                    .map(|y| y as i32)
                    .unwrap_or(-1);

                for y in 0..LEGACY_CHUNK_HEIGHT_1_16_5 {
                    let world_y = y as i32;
                    let state = if world_y == 0 {
                        &bedrock
                    } else if world_y <= top && self.is_solid(x, y, z) == Some(true) {
                        if world_y == top {
                            &grass
                        } else if world_y >= top - 3 {
                            &dirt
                        } else {
                            &stone
                        }
                    } else if world_y > top && world_y <= sea_level {
                        &water
                    } else {
                        &air
                    };
                    sections[y / 16].set(x, y % 16, z, state.clone())?;
                }
            }
        }
        Ok(sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_complete_finite_chunk_density_volume() {
        let noise = LegacySurfaceNoise1_16_5::from_seed(12345);
        let chunk = LegacyTerrainDensityChunk1_16_5::generate(&noise, 0, 0);
        assert_eq!(chunk.densities().len(), LEGACY_CHUNK_DENSITY_COUNT_1_16_5);
        assert!(chunk.densities().iter().all(|value| value.is_finite()));
        assert!(chunk.density(0, 0, 0).is_some());
        assert!(chunk.density(15, 255, 15).is_some());
        assert_eq!(chunk.density(16, 0, 0), None);
        assert_eq!(chunk.density(0, 256, 0), None);
    }

    #[test]
    fn generation_is_repeatable_for_the_same_seed_and_chunk() {
        let noise = LegacySurfaceNoise1_16_5::from_seed(-987654321);
        let first = LegacyTerrainDensityChunk1_16_5::generate(&noise, -2, 3);
        let second = LegacyTerrainDensityChunk1_16_5::generate(&noise, -2, 3);
        assert_eq!(first, second);
    }

    #[test]
    fn density_materializes_into_legacy_surface_layers_and_water() {
        let mut chunk = LegacyTerrainDensityChunk1_16_5 {
            x: 0,
            z: 0,
            densities: vec![-1.0; LEGACY_CHUNK_DENSITY_COUNT_1_16_5],
        };
        for y in 0..=10 {
            for z in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
                for x in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
                    let index = (y * LEGACY_CHUNK_WIDTH_1_16_5 + z) * LEGACY_CHUNK_WIDTH_1_16_5 + x;
                    chunk.densities[index] = 1.0;
                }
            }
        }

        let sections = chunk.to_legacy_sections(63).unwrap();
        let block = |x: usize, y: usize, z: usize| {
            sections[y / 16].get(x, y % 16, z).unwrap().name.as_str()
        };
        assert_eq!(sections.len(), 16);
        assert_eq!(block(0, 0, 0), "bedrock");
        assert_eq!(block(0, 10, 0), "grass_block");
        assert_eq!(block(0, 9, 0), "dirt");
        assert_eq!(block(0, 7, 0), "dirt");
        assert_eq!(block(0, 6, 0), "stone");
        assert_eq!(block(0, 11, 0), "water");
        assert_eq!(block(0, 63, 0), "water");
        assert_eq!(block(0, 64, 0), "air");
    }

    #[test]
    fn chunk_coordinates_are_preserved() {
        let noise = LegacySurfaceNoise1_16_5::from_seed(42);
        let chunk = LegacyTerrainDensityChunk1_16_5::generate(&noise, -7, 11);
        assert_eq!((chunk.x, chunk.z), (-7, 11));
    }
}
