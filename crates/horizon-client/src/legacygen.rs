//! First runtime adapter for the Minecraft Java 1.16.5 terrain prototype.
//!
//! This generator deliberately covers only the base density surface. It does
//! not claim vanilla parity: biome shaping, caves, structures, surface rules
//! and decoration are separate follow-up stages.
use crate::{
    chunk::{Chunk, BIOME_STRATEGY, MIN_Y, SECTIONS},
    streaming::{ChunkGenerator, Proto},
};
use horizon_core::paletted::PalettedContainer;
use horizon_world::{
    legacy_surface_noise_1_16_5::LegacySurfaceNoise1_16_5,
    legacy_terrain_density_chunk_1_16_5::{
        LegacyTerrainDensityChunk1_16_5, LEGACY_CHUNK_HEIGHT_1_16_5,
        LEGACY_CHUNK_WIDTH_1_16_5,
    },
    registry::BlockRegistry,
};
use std::sync::Arc;

/// Minimal 1.16.5 base-terrain generator for the live chunk streaming API.
pub struct LegacyTerrain {
    noise: LegacySurfaceNoise1_16_5,
    bedrock: u32,
    stone: u32,
    dirt: u32,
    grass: u32,
    water: u32,
    plains: u16,
}

impl LegacyTerrain {
    /// Resolve the active renderer's block IDs once, before workers start.
    pub fn new(
        registry: &BlockRegistry,
        biomes: &crate::biome::Biomes,
        seed: i64,
    ) -> crate::Result<Self> {
        let state = |name: &str| -> crate::Result<u32> {
            Ok(registry
                .default_state(name)
                .ok_or_else(|| format!("missing block state {name}"))?
                .0)
        };
        Ok(Self {
            noise: LegacySurfaceNoise1_16_5::from_seed(seed),
            bedrock: state("minecraft:bedrock")?,
            stone: state("minecraft:stone")?,
            dirt: state("minecraft:dirt")?,
            grass: state("minecraft:grass_block")?,
            water: state("minecraft:water")?,
            plains: biomes.id("minecraft:plains"),
        })
    }

    fn generate(&self, chunk_x: i32, chunk_z: i32) -> Chunk {
        let density = LegacyTerrainDensityChunk1_16_5::generate(
            &self.noise,
            chunk_x,
            chunk_z,
        );
        let mut chunk = Chunk::empty(chunk_x, chunk_z);

        // 1.16.5 build height is Y=0..255. Its sections occupy resident
        // slots 4..19 because the renderer's current storage starts at Y=-64.
        for section_y in 4..20.min(SECTIONS) {
            Arc::make_mut(&mut chunk.sections[section_y]).biomes =
                PalettedContainer::new(BIOME_STRATEGY, u32::from(self.plains));
        }

        for z in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
            for x in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
                let top = (0..LEGACY_CHUNK_HEIGHT_1_16_5)
                    .rev()
                    .find(|&y| density.is_solid(x, y, z) == Some(true))
                    .map(|y| y as i32)
                    .unwrap_or(-1);

                let highest = top.max(63).min(255);
                for y in 0..=highest {
                    let state = if y == 0 {
                        self.bedrock
                    } else if y <= top {
                        if y == top {
                            self.grass
                        } else if y >= top - 3 {
                            self.dirt
                        } else {
                            self.stone
                        }
                    } else if y <= 63 {
                        self.water
                    } else {
                        continue;
                    };
                    chunk.set(
                        chunk_x * 16 + x as i32,
                        y,
                        chunk_z * 16 + z as i32,
                        state,
                    );
                }
            }
        }
        chunk
    }
}

impl ChunkGenerator for LegacyTerrain {
    type Worker = ();

    fn worker(&self) {}

    fn proto(&self, _worker: &mut (), x: i32, z: i32) -> Proto {
        Box::new(self.generate(x, z))
    }

    fn finish(&self, proto: Proto) -> Chunk {
        let mut chunk = *proto
            .downcast::<Chunk>()
            .expect("legacy 1.16.5 terrain proto");
        for section in &mut chunk.sections {
            Arc::make_mut(section).compact();
        }
        chunk.fresh = true;
        chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_world_sections_land_in_the_correct_resident_slots() {
        assert_eq!((0 - MIN_Y) / 16, 4);
        assert_eq!((240 - MIN_Y) / 16, 19);
        assert_eq!(LEGACY_CHUNK_HEIGHT_1_16_5, 256);
    }

    #[test]
    fn legacy_density_chunk_uses_the_requested_chunk_coordinates() {
        let noise = LegacySurfaceNoise1_16_5::from_seed(1234);
        let chunk = LegacyTerrainDensityChunk1_16_5::generate(&noise, -3, 7);
        assert_eq!((chunk.x, chunk.z), (-3, 7));
        assert!(chunk.density(0, 64, 0).is_some());
    }
}
