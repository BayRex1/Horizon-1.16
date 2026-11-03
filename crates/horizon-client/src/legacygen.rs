//! First runtime adapter for the Minecraft Java 1.16.5 terrain prototype.
//!
//! This generator deliberately covers only the base density surface. It does
//! not claim vanilla parity: biome shaping, caves, structures, surface rules
//! and decoration are separate follow-up stages.
use crate::{
    chunk::{BIOME_STRATEGY, Chunk, SECTIONS, block_strategy},
    lod::Surface,
    streaming::{ChunkGenerator, Proto},
};
use horizon_core::paletted::PalettedContainer;
use horizon_world::{
    legacy_surface_noise_1_16_5::LegacySurfaceNoise1_16_5,
    legacy_terrain_density_chunk_1_16_5::{
        LEGACY_CHUNK_HEIGHT_1_16_5, LEGACY_CHUNK_WIDTH_1_16_5, LegacyTerrainDensityChunk1_16_5,
    },
    registry::BlockRegistry,
};
use std::sync::Arc;

/// One worker keeps its most recently sampled density chunk. The same density
/// volume feeds both near terrain and far-terrain LOD, avoiding a second noise
/// evaluation when the streamer asks for the same chunk's surface.
#[derive(Default)]
pub struct LegacyTerrainWorker {
    cached: Option<(i32, i32, LegacyTerrainDensityChunk1_16_5)>,
    surface_heights: Option<(i32, i32, [i16; 256])>,
}

/// Minimal 1.16.5 base-terrain generator for the live chunk streaming API.
pub struct LegacyTerrain {
    noise: LegacySurfaceNoise1_16_5,
    air: u32,
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
            air: state("minecraft:air")?,
            bedrock: state("minecraft:bedrock")?,
            stone: state("minecraft:stone")?,
            dirt: state("minecraft:dirt")?,
            grass: state("minecraft:grass_block")?,
            water: state("minecraft:water")?,
            plains: biomes.id("minecraft:plains"),
        })
    }

    fn density<'a>(
        &self,
        worker: &'a mut LegacyTerrainWorker,
        chunk_x: i32,
        chunk_z: i32,
    ) -> &'a LegacyTerrainDensityChunk1_16_5 {
        if worker
            .cached
            .as_ref()
            .is_none_or(|(x, z, _)| *x != chunk_x || *z != chunk_z)
        {
            worker.cached = Some((
                chunk_x,
                chunk_z,
                LegacyTerrainDensityChunk1_16_5::generate(&self.noise, chunk_x, chunk_z),
            ));
        }
        &worker.cached.as_ref().expect("density cache populated").2
    }

    fn surface_heights(density: &LegacyTerrainDensityChunk1_16_5) -> [i16; 256] {
        std::array::from_fn(|column| {
            let x = column & 15;
            let z = column >> 4;
            (0..LEGACY_CHUNK_HEIGHT_1_16_5)
                .rev()
                .find(|&y| density.is_solid(x, y, z) == Some(true))
                .map_or(-1, |y| y as i16)
        })
    }

    fn surface(&self, top: i16) -> Option<Surface> {
        let top = i32::from(top);
        (top >= 0).then_some(Surface {
            height: top + 1,
            state: horizon_world::registry::BlockStateId(self.grass),
            water: (top < 63).then_some((64, horizon_world::registry::BlockStateId(self.water))),
            biome: self.plains,
        })
    }

    /// Creates resident storage with the registry's actual air state.
    ///
    /// Do not assume state ID zero is air: the legacy generator is bound to
    /// the active resource registry, whose IDs are not the 1.16.5 protocol IDs.
    fn empty_chunk(&self, chunk_x: i32, chunk_z: i32) -> Chunk {
        let mut chunk = Chunk::empty(chunk_x, chunk_z);
        for section in &mut chunk.sections {
            let section = Arc::make_mut(section);
            section.blocks = PalettedContainer::new(block_strategy(), self.air);
        }

        // 1.16.5 build height is Y=0..255. Its sections occupy resident
        // slots 4..19 because the renderer's current storage starts at Y=-64.
        for section_y in 4..20.min(SECTIONS) {
            Arc::make_mut(&mut chunk.sections[section_y]).biomes =
                PalettedContainer::new(BIOME_STRATEGY, u32::from(self.plains));
        }
        chunk
    }

    fn generate(
        &self,
        density: &LegacyTerrainDensityChunk1_16_5,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Chunk {
        let mut chunk = self.empty_chunk(chunk_x, chunk_z);

        for z in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
            for x in 0..LEGACY_CHUNK_WIDTH_1_16_5 {
                let top = (0..LEGACY_CHUNK_HEIGHT_1_16_5)
                    .rev()
                    .find(|&y| density.is_solid(x, y, z) == Some(true))
                    .map(|y| y as i32)
                    .unwrap_or(-1);

                let highest = top.clamp(63, 255);
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
                    chunk.set(chunk_x * 16 + x as i32, y, chunk_z * 16 + z as i32, state);
                }
            }
        }
        chunk
    }
}

impl ChunkGenerator for LegacyTerrain {
    type Worker = LegacyTerrainWorker;

    fn worker(&self) -> Self::Worker {
        LegacyTerrainWorker::default()
    }

    fn lod_surface(&self, worker: &mut Self::Worker, x: i32, z: i32) -> Option<Surface> {
        let chunk_x = x.div_euclid(16);
        let chunk_z = z.div_euclid(16);
        let local_x = x.rem_euclid(16) as usize;
        let local_z = z.rem_euclid(16) as usize;
        let needs_surface_cache = worker
            .surface_heights
            .as_ref()
            .is_none_or(|(x, z, _)| *x != chunk_x || *z != chunk_z);
        if needs_surface_cache {
            let heights = {
                let density = self.density(worker, chunk_x, chunk_z);
                Self::surface_heights(density)
            };
            worker.surface_heights = Some((chunk_x, chunk_z, heights));
        }
        let heights = &worker
            .surface_heights
            .as_ref()
            .expect("surface cache populated")
            .2;
        self.surface(heights[local_z * 16 + local_x])
    }

    fn proto(&self, worker: &mut Self::Worker, x: i32, z: i32) -> Proto {
        let density = self.density(worker, x, z);
        Box::new(self.generate(density, x, z))
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
    use crate::chunk::MIN_Y;

    #[test]
    fn resident_chunk_starts_with_the_resolved_air_state() {
        let terrain = LegacyTerrain {
            noise: LegacySurfaceNoise1_16_5::from_seed(17),
            air: 91,
            bedrock: 1,
            stone: 2,
            dirt: 3,
            grass: 4,
            water: 5,
            plains: 6,
        };
        let chunk = terrain.empty_chunk(-2, 3);

        assert_eq!(chunk.get(-32, 100, 48), 91);
        assert_eq!(chunk.get(-32, 0, 48), 91);
        assert_eq!(chunk.get(-32, 255, 48), 91);
        assert_eq!(chunk.sections[4].biomes.get(0), u32::from(terrain.plains));
        assert_eq!(chunk.sections[19].biomes.get(63), u32::from(terrain.plains));
    }

    #[test]
    fn legacy_world_sections_land_in_the_correct_resident_slots() {
        assert_eq!((0 - MIN_Y) / 16, 4);
        assert_eq!((240 - MIN_Y) / 16, 19);
        assert_eq!(LEGACY_CHUNK_HEIGHT_1_16_5, 256);
    }

    #[test]
    fn generated_chunk_contains_the_sampled_legacy_terrain() {
        let terrain = LegacyTerrain {
            noise: LegacySurfaceNoise1_16_5::from_seed(0x16_5),
            air: 0,
            bedrock: 1,
            stone: 2,
            dirt: 3,
            grass: 4,
            water: 5,
            plains: 6,
        };
        let chunk_x = -2;
        let chunk_z = 3;
        let density = LegacyTerrainDensityChunk1_16_5::generate(
            &terrain.noise,
            chunk_x,
            chunk_z,
        );
        let chunk = terrain.generate(&density, chunk_x, chunk_z);

        assert_eq!(chunk.get(chunk_x * 16, 0, chunk_z * 16), terrain.bedrock);
        let column = (0..256)
            .find_map(|column| {
                let x = column & 15;
                let z = column >> 4;
                let top = (0..LEGACY_CHUNK_HEIGHT_1_16_5)
                    .rev()
                    .find(|&y| density.is_solid(x, y, z) == Some(true))?;
                (top >= 4).then_some((x, z, top as i32))
            })
            .expect("seed should produce at least one solid surface column");
        let (x, z, top) = column;
        let world_x = chunk_x * 16 + x as i32;
        let world_z = chunk_z * 16 + z as i32;

        assert_eq!(chunk.get(world_x, top, world_z), terrain.grass);
        assert_eq!(chunk.get(world_x, top - 1, world_z), terrain.dirt);
        assert_eq!(chunk.get(world_x, top - 3, world_z), terrain.dirt);
        assert_eq!(chunk.get(world_x, top - 4, world_z), terrain.stone);
        if top < 63 {
            assert_eq!(chunk.get(world_x, 63, world_z), terrain.water);
        }
    }

    #[test]
    fn legacy_density_chunk_uses_the_requested_chunk_coordinates() {
        let noise = LegacySurfaceNoise1_16_5::from_seed(1234);
        let chunk = LegacyTerrainDensityChunk1_16_5::generate(&noise, -3, 7);
        assert_eq!((chunk.x, chunk.z), (-3, 7));
        assert!(chunk.density(0, 64, 0).is_some());
    }

    #[test]
    fn lod_surface_uses_block_coordinates_and_reuses_the_chunk_density() {
        let terrain = LegacyTerrain {
            noise: LegacySurfaceNoise1_16_5::from_seed(9876),
            air: 0,
            bedrock: 1,
            stone: 2,
            dirt: 3,
            grass: 4,
            water: 5,
            plains: 6,
        };
        let mut worker = terrain.worker();

        // -1, 16 is local block (15, 0) in chunk (-1, 1).
        let surface = terrain.lod_surface(&mut worker, -1, 16).unwrap();
        assert_eq!(
            worker.cached.as_ref().map(|(x, z, _)| (*x, *z)),
            Some((-1, 1))
        );
        assert_eq!(
            worker.surface_heights.as_ref().map(|(x, z, _)| (*x, *z)),
            Some((-1, 1))
        );

        let density = terrain.density(&mut worker, -1, 1);
        let top = (0..LEGACY_CHUNK_HEIGHT_1_16_5)
            .rev()
            .find(|&y| density.is_solid(15, y, 0) == Some(true))
            .unwrap() as i32;
        assert_eq!(surface.height, top + 1);
        assert_eq!(worker.surface_heights.as_ref().unwrap().2[15], top as i16);
        assert_eq!(surface.state.0, terrain.grass);
        assert_eq!(surface.biome, terrain.plains);
        assert_eq!(
            surface.water.map(|(height, state)| (height, state.0)),
            (top < 63).then_some((64, terrain.water))
        );
    }
}
