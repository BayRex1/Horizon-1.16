//! A server's chunk columns (`ClientboundLevelChunkWithLightPacket`): the
//! sections and light of each packet become a client column on a decoder
//! thread as soon as the packet arrives, so the frame that handles it in
//! order only inserts the result (`ClientPacketListener.handleLevelChunkWithLight`).
use super::Game;
use horizon_client::{
    chunk::{BIOME_STRATEGY, Chunk, Light, MIN_Y, SECTIONS, Section, block_strategy},
    streaming::Streaming,
};
use horizon_core::paletted::{PalettedContainer, Strategy};
use horizon_protocol::{
    chunk::{LightData, SectionLight},
    packets::play::{self, Clientbound as C},
    registry::DimensionType,
};
use std::sync::{
    Arc,
    mpsc::{self, TryRecvError},
};

/// What turning a chunk packet into a client column depends on: the
/// dimension's height and sky, and the server's biome registry mapped to
/// ours. The connection replaces it when either changes, so a column decoded
/// with an older one is decoded again.
#[derive(Default)]
pub(super) struct ChunkFormat {
    min_y: i32,
    sections: usize,
    has_skylight: bool,
    biome_map: Vec<u16>,
    biome_bits: u8,
}

impl ChunkFormat {
    pub(super) fn new(dimension: &DimensionType, biome_map: Vec<u16>, biome_bits: u8) -> Self {
        Self {
            min_y: dimension.min_y,
            sections: dimension.sections(),
            has_skylight: dimension.has_skylight,
            biome_map,
            biome_bits,
        }
    }
    pub(super) fn with_dimension(&self, dimension: &DimensionType) -> Self {
        Self::new(dimension, self.biome_map.clone(), self.biome_bits)
    }

    /// LevelChunkSection data into the client's fixed -64..320 column.
    fn column(&self, p: &play::LevelChunkWithLight) -> Result<Chunk, String> {
        let sections = p
            .chunk
            .sections(self.sections, 16, self.biome_bits)
            .map_err(|e| e.to_string())?;
        let mut chunk = Chunk::empty(p.x, p.z);
        let offset = (self.min_y - MIN_Y).div_euclid(16);
        // Biome table index per quart cell of the dimension's own sections.
        let mut biome_cells: Vec<[u32; 64]> = Vec::with_capacity(sections.len());
        for (i, s) in sections.iter().enumerate() {
            let target = offset + i as i32;
            let mut cells = [0u32; 64];
            for (j, cell) in cells.iter_mut().enumerate() {
                *cell = u32::from(
                    self.biome_map
                        .get(s.biomes.get(j) as usize)
                        .copied()
                        .unwrap_or(0),
                );
            }
            biome_cells.push(cells);
            if !(0..SECTIONS as i32).contains(&target) {
                continue;
            }
            // A global section is as wide as the server's registry: 16 bits
            // from a vanilla server, wider from a Horizon host with the same
            // content mods. Read it at that width, then widen it to ours.
            let ours = block_strategy();
            let strategy = if ours.wire_has_palette(s.states.bits) {
                ours
            } else {
                Strategy::blocks(s.states.bits)
            };
            let palette = s.states.palette.iter().map(|&p| p as u32).collect();
            let words = s.states.data.iter().map(|&w| w as u64).collect();
            let mut blocks = PalettedContainer::from_wire(strategy, s.states.bits, palette, words)
                .map_err(|e| e.to_string())?;
            if strategy.global_bits != ours.global_bits {
                let mut values = [0u32; 4096];
                blocks.unpack(&mut values);
                // Ids past our registry (a host with mods we lack) read as air.
                let limit = 1u64 << ours.global_bits;
                for v in &mut values {
                    if u64::from(*v) >= limit {
                        *v = 0;
                    }
                }
                blocks = PalettedContainer::from_values(ours, &values);
            }
            let mut section = Section {
                blocks,
                biomes: PalettedContainer::from_values(BIOME_STRATEGY, &cells),
                light: Light::Uniform(0xF0),
            };
            section.compact();
            chunk.sections[target as usize] = Arc::new(section);
        }
        // Sections outside the dimension's range take its nearest biomes
        // (the column's top or bottom cell layer repeated).
        if !biome_cells.is_empty() {
            let last = biome_cells.len() - 1;
            for si in 0..SECTIONS as i32 {
                let i = si - offset;
                if (0..sections.len() as i32).contains(&i) {
                    continue;
                }
                let (edge, layer) = if i < 0 { (0, 0) } else { (last, 3) };
                let mut cells = [0u32; 64];
                for (j, cell) in cells.iter_mut().enumerate() {
                    *cell = biome_cells[edge][layer * 16 + j % 16];
                }
                Arc::make_mut(&mut chunk.sections[si as usize]).biomes =
                    PalettedContainer::from_values(BIOME_STRATEGY, &cells);
            }
        }
        // ClientPacketListener.applyLightData: the column keeps the light
        // the server sent; nothing is relit.
        let (layers, above) = self.light(&p.light);
        horizon_client::light::light_from_layers(&mut chunk, &layers, above, self.has_skylight);
        Ok(chunk)
    }

    /// `ClientPacketListener.readSectionList`: the sky and block layers of a
    /// light packet, by section of the client's column, and the sky light
    /// above the column's top section (from the light section just above
    /// it, else open sky).
    pub(super) fn light(&self, light: &LightData) -> (Vec<SectionLight>, [u8; 256]) {
        let mut layers: Vec<SectionLight> = vec![(None, None); SECTIONS];
        let sky = self.has_skylight;
        let mut above = [if sky { 15 } else { 0 }; 256];
        // Light section b is the dimension's section b - 1.
        let first = (self.min_y - MIN_Y).div_euclid(16) - 1;
        for (b, layer) in light.layers(self.sections + 2).into_iter().enumerate() {
            let si = first + b as i32;
            if (0..SECTIONS as i32).contains(&si) {
                layers[si as usize] = layer;
            } else if si == SECTIONS as i32
                && sky
                && let Some(l) = &layer.0
            {
                for (i, a) in above.iter_mut().enumerate() {
                    *a = (l[i >> 1] >> ((i & 1) * 4)) & 15;
                }
            }
        }
        (layers, above)
    }
}

type Decoded = (Box<C>, Result<Chunk, String>);

/// Convert one legacy 1.16.5 section's biome slice into the resident
/// section's quart-cell order. Protocol 754 stores biomes as
/// `(quart_x * 4 + quart_z) * 64 + quart_y`; resident sections use
/// `(quart_y << 4) | (quart_z << 2) | quart_x`.
fn legacy_biome_cells(
    chunk: &horizon_protocol::legacy_1_16_5::LegacyChunkColumn,
    legacy_y: usize,
    biome_map: &[u32],
) -> std::result::Result<[u32; 64], String> {
    let mut cells = [0u32; 64];
    if !chunk.full_chunk || chunk.biomes.len() != 1024 || legacy_y >= 16 {
        return Ok(cells);
    }
    for quart_y in 0..4 {
        for quart_z in 0..4 {
            for quart_x in 0..4 {
                let source_index = (quart_x * 4 + quart_z) * 64 + legacy_y * 4 + quart_y;
                let target_index = (quart_y << 4) | (quart_z << 2) | quart_x;
                let legacy_biome = chunk.biomes[source_index];
                cells[target_index] = if legacy_biome >= 0 {
                    *biome_map.get(legacy_biome as usize).ok_or_else(|| {
                        format!("missing protocol-754 biome mapping for ID {legacy_biome}")
                    })?
                } else {
                    return Err(format!("invalid protocol-754 biome ID {legacy_biome}"));
                };
            }
        }
    }
    cells
}

/// One thread that decodes chunk packets in the order they arrived; its
/// results come back in that order, one per `submit`.
pub(super) struct ChunkDecoder {
    jobs: mpsc::Sender<(Arc<ChunkFormat>, Box<C>)>,
    done: mpsc::Receiver<Decoded>,
}

impl ChunkDecoder {
    pub(super) fn new() -> Self {
        let (jobs, todo) = mpsc::channel::<(Arc<ChunkFormat>, Box<C>)>();
        let (finished, done) = mpsc::channel();
        // The thread ends with the connection, when `jobs` is dropped.
        let spawned = std::thread::Builder::new()
            .name("Chunk decoder".into())
            .spawn(move || {
                // A frame waits for its columns, as it would for the render
                // thread's own work.
                horizon_threads::favour_current();
                for (format, packet) in todo {
                    let column = match &*packet {
                        C::LevelChunkWithLight(p) => format.column(p),
                        _ => Err("not a chunk packet".into()),
                    };
                    if finished.send((packet, column)).is_err() {
                        break;
                    }
                }
            });
        if let Err(e) = spawned {
            log::warn!("Horizon: no chunk decoder thread ({e}); chunks decode in the frame");
        }
        Self { jobs, done }
    }

    /// Start decoding a chunk packet with the format it arrived under.
    /// Without a decoder thread the packet comes back undecoded.
    pub(super) fn submit(&self, format: Arc<ChunkFormat>, packet: Box<C>) -> Option<Box<C>> {
        self.jobs.send((format, packet)).err().map(|e| (e.0).1)
    }

    /// The oldest submitted packet and its column, unless it is still
    /// decoding: a frame never waits for the decoder.
    pub(super) fn next(&self) -> Next {
        match self.done.try_recv() {
            Ok(decoded) => Next::Ready(decoded),
            Err(TryRecvError::Empty) => Next::Decoding,
            // The decoder thread panicked: its columns are gone.
            Err(TryRecvError::Disconnected) => Next::Lost,
        }
    }
}

pub(super) enum Next {
    Ready(Decoded),
    Decoding,
    Lost,
}

impl Game<'_> {
    /// Route one decoded protocol-754 Play event into the resident world.
    ///
    /// The network loop can hand events here without knowing about renderer
    /// internals. Position, disconnect and keep-alive events are handled by
    /// the protocol layer; only chunk payloads enter the world-storage path.
    #[allow(dead_code)]
    pub(super) fn remote_legacy_play_event(
        &mut self,
        event: horizon_protocol::legacy_1_16_5::PlayEvent,
        block_state_map: &[u32],
        biome_map: &[u32],
    ) -> std::result::Result<(), String> {
        if let horizon_protocol::legacy_1_16_5::PlayEvent::ChunkReceived { column } = event {
            self.remote_legacy_chunk(column, block_state_map, biome_map)?;
        }
        Ok(())
    }

    /// Install a decoded protocol-754 chunk into resident world storage.
    ///
    /// This is the renderer bridge for the legacy decoder. The 1.16.5 block
    /// state IDs are version-specific, so callers must only enable this path
    /// after the 1.16.5 registry has been mapped to the client's render IDs.
    /// Until that registry mapping exists, accepting arbitrary legacy IDs
    /// would render unrelated modern blocks.
    // The protocol-754 packet dispatch is not wired into the live game loop yet.
    // Keep this bridge available for that integration without failing workspace clippy.
    #[allow(dead_code)]
    pub(super) fn remote_legacy_chunk(
        &mut self,
        chunk: horizon_protocol::legacy_1_16_5::LegacyChunkColumn,
        block_state_map: &[u32],
        biome_map: &[u32],
    ) -> std::result::Result<(), String> {
        if !self.remote_chunk_in_range(chunk.x, chunk.z) {
            return Ok(());
        }

        // Fail closed instead of silently turning unknown legacy blocks into
        // air. A partial ID map would otherwise create a plausible but corrupt
        // looking world and hide registry incompatibilities.
        for source in chunk.sections.iter().flatten() {
            for index in 0..4096 {
                let legacy_id = source.block_state(index).unwrap_or(0);
                if legacy_id < 0 || block_state_map.get(legacy_id as usize).is_none() {
                    return Err(format!(
                        "missing protocol-754 block-state mapping for ID {legacy_id}"
                    ));
                }
            }
        }
        if chunk.full_chunk && chunk.biomes.len() == 1024 {
            for &legacy_biome in &chunk.biomes {
                if legacy_biome < 0 || biome_map.get(legacy_biome as usize).is_none() {
                    return Err(format!(
                        "missing protocol-754 biome mapping for ID {legacy_biome}"
                    ));
                }
            }
        }

        let mut column = Chunk::empty(chunk.x, chunk.z);
        // Java 1.16.5 has sections Y=0..255. Horizon's resident columns span
        // -64..319, so legacy section 0 is stored at resident section index 4.
        const LEGACY_SECTION_OFFSET: usize = ((0 - MIN_Y) / 16) as usize;
        for (legacy_y, source) in chunk.sections.iter().enumerate() {
            let Some(source) = source else { continue };
            let target_y = LEGACY_SECTION_OFFSET + legacy_y;
            if target_y >= column.sections.len() {
                continue;
            }
            let mut values = [0u32; 4096];
            for (index, value) in values.iter_mut().enumerate() {
                let legacy_id = source.block_state(index).unwrap_or(0);
                *value = block_state_map[legacy_id as usize];
            }
            let biome_cells = legacy_biome_cells(&chunk, legacy_y, biome_map)?;
            let mut section = Section {
                blocks: PalettedContainer::from_values(block_strategy(), &values),
                biomes: PalettedContainer::from_values(BIOME_STRATEGY, &biome_cells),
                light: Light::Uniform(0xF0),
            };
            section.compact();
            column.sections[target_y] = Arc::new(section);
        }

        self.streaming.insert_remote(&mut self.world, column);
        self.world
            .block_entities
            .retain(|k, _| (k.0 >> 4, k.2 >> 4) != (chunk.x, chunk.z));
        Ok(())
    }

    /// `handleLevelChunkWithLight`: the column replaces any loaded one, and
    /// its block entities load their update tags. `decoded` is the column the
    /// decoder made with `format`; another format since means decode again.
    pub(super) fn remote_chunk(
        &mut self,
        p: &play::LevelChunkWithLight,
        decoded: Option<(&Arc<ChunkFormat>, Result<Chunk, String>)>,
    ) {
        // ClientChunkCache.replaceWithPacketData: a column outside the
        // storage around the view centre is ignored.
        if !self.remote_chunk_in_range(p.x, p.z) {
            log::warn!(
                "Ignoring chunk since it's not in the view range: {}, {}",
                p.x,
                p.z
            );
            return;
        }
        let Some(format) = self.remote.as_ref().map(|r| r.chunk_format.clone()) else {
            return;
        };
        let column = match decoded {
            Some((used, column)) if Arc::ptr_eq(used, &format) => column,
            _ => format.column(p),
        };
        let chunk = match column {
            Ok(chunk) => chunk,
            Err(e) => {
                log::warn!("Horizon: chunk {} {}: {e}", p.x, p.z);
                return;
            }
        };
        let (x, z) = (p.x, p.z);
        self.streaming.insert_remote(&mut self.world, chunk);
        self.world
            .block_entities
            .retain(|k, _| (k.0 >> 4, k.2 >> 4) != (x, z));
        for info in &p.chunk.block_entities {
            let pos = (
                x * 16 + i32::from(info.packed_xz as u8 >> 4),
                i32::from(info.y),
                z * 16 + i32::from(info.packed_xz as u8 & 15),
            );
            let state = self.world.block(pos.0, pos.1, pos.2);
            self.sync_block_entity(pos, state);
            // LevelChunk.replaceWithPacketData: the block entity loads its update tag.
            self.remote_block_entity_data([pos.0, pos.1, pos.2], info.kind, &info.tag);
        }
    }

    /// `handleLightUpdatePacket`: the sections it names take its light.
    pub(super) fn remote_light_update(&mut self, p: &play::LightUpdate) {
        let Some(format) = self.remote.as_ref().map(|r| r.chunk_format.clone()) else {
            return;
        };
        if !self.world.chunks.contains_key(&(p.x, p.z)) {
            return;
        }
        let (layers, _) = format.light(&p.light);
        let Some(chunk) = self.world.chunks.get_mut(&(p.x, p.z)) else {
            return;
        };
        let changed = horizon_client::light::update_light_layers(
            Arc::make_mut(chunk),
            &layers,
            format.has_skylight,
        );
        Streaming::light_updated(&mut self.world, p.x, p.z, &changed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_protocol::chunk::{ChunkData, Paletted, Section as Wire, Strategy as Wiring};

    /// A column of `state` with biome 1 everywhere, unlit.
    fn packet(x: i32, state: i32) -> Box<C> {
        let section = Wire {
            non_empty: 4096,
            fluids: 0,
            states: Paletted::from_values(&[state; 4096], Wiring::Blocks, 16),
            biomes: Paletted::from_values(&[1; 64], Wiring::Biomes, 2),
        };
        let none = LightData {
            sky_mask: Vec::new(),
            block_mask: Vec::new(),
            empty_sky_mask: Vec::new(),
            empty_block_mask: Vec::new(),
            sky: Vec::new(),
            block: Vec::new(),
        };
        Box::new(C::LevelChunkWithLight(play::LevelChunkWithLight {
            x,
            z: -3,
            chunk: ChunkData {
                heightmaps: Vec::new(),
                buffer: ChunkData::from_sections(&vec![section; 24]),
                block_entities: Vec::new(),
            },
            light: none,
        }))
    }

    #[test]
    fn legacy_biomes_are_transposed_into_resident_quart_order() {
        use horizon_protocol::legacy_1_16_5::LegacyChunkColumn;
        let mut column = LegacyChunkColumn {
            x: 0,
            z: 0,
            full_chunk: true,
            section_mask: 0xffff,
            heightmaps: horizon_world::nbt::Tag::Compound(Default::default()),
            biomes: vec![0; 1024],
            sections: (0..16).map(|_| None).collect(),
            block_entities: Vec::new(),
        };
        // Unique IDs reveal transposition mistakes and the Y-section offset.
        for quart_x in 0..4 {
            for quart_z in 0..4 {
                for quart_y in 0..64 {
                    let source = (quart_x * 4 + quart_z) * 64 + quart_y;
                    column.biomes[source] = quart_y as i32 + 1;
                }
            }
        }
        let map: Vec<u32> = (0..32).map(|v| v * 3).collect();
        let cells = legacy_biome_cells(&column, 5, &map).unwrap();
        for quart_y in 0..4 {
            for quart_z in 0..4 {
                for quart_x in 0..4 {
                    let target = (quart_y << 4) | (quart_z << 2) | quart_x;
                    assert_eq!(cells[target], map[5 * 4 + quart_y + 1]);
                }
            }
        }
        assert_eq!(legacy_biome_cells(&column, 16, &map).unwrap(), [0; 64]);
        assert!(legacy_biome_cells(&column, 5, &[0]).is_err());
        column.full_chunk = false;
        assert_eq!(legacy_biome_cells(&column, 5, &map).unwrap(), [0; 64]);
    }

    /// Columns come back one per packet, in the order the packets went in,
    /// as the frame would have decoded them itself.
    #[test]
    fn the_decoder_returns_columns_in_order() {
        let overworld = DimensionType::vanilla("minecraft:overworld");
        let format = Arc::new(ChunkFormat::new(&overworld, vec![0, 7, 9], 2));
        let decoder = ChunkDecoder::new();
        for x in 0..20 {
            assert!(decoder.submit(format.clone(), packet(x, 1 + x)).is_none());
        }
        for x in 0..20 {
            let (packet, column) = loop {
                match decoder.next() {
                    Next::Ready(decoded) => break decoded,
                    Next::Decoding => std::thread::yield_now(),
                    Next::Lost => panic!("the decoder stopped"),
                }
            };
            let C::LevelChunkWithLight(p) = *packet else {
                panic!("not a chunk packet");
            };
            let (column, direct) = (column.unwrap(), format.column(&p).unwrap());
            assert_eq!((column.x, column.z), (x, -3));
            for (cx, y, cz) in [(0, -64, 0), (5, 70, 9), (15, 319, 15)] {
                assert_eq!(column.get(cx, y, cz), 1 + x as u32);
                assert_eq!(column.get(cx, y, cz), direct.get(cx, y, cz));
                // Wire biome 1 is our biome 7.
                assert_eq!(column.biome(cx, y, cz), 7);
            }
        }
        assert!(matches!(decoder.next(), Next::Decoding));
    }
}
