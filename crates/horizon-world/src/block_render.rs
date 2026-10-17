//! Neighbor face visibility over immutable block definitions and occlusion grids.
//! This decides whether a culled model face is emitted; it does not shade it or
//! implement translucent sorting, fluid meshing, or block updates.
use crate::{
    collision::CachedCollisionRegistry,
    compression::invalid,
    fluid::{FluidKind, FluidState},
    registry::BlockStateId,
};
use horizon_core::geometry::Axis;
use horizon_core::geometry::{Direction, ShapeOp, VoxelShape};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    io,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipRendering {
    Never,
    SameBlock,
    Bars,
    Leaves,
    Liquid,
    MangroveRoots,
}
#[derive(Clone, Copy, Debug)]
pub struct RenderDefinition {
    pub skip: SkipRendering,
    pub can_occlude: bool,
    pub bars_tag: bool,
}
#[derive(Clone, Copy)]
struct State {
    block: BlockStateId,
    faces: [u32; 6],
    definition: RenderDefinition,
    connections: u8,
    connection_properties: u8,
    fluid: FluidKind,
}
/// Prepared face geometry and a bounded cache shared across state pairs. Geometry
/// is deduplicated by the imported immutable shape, not copied for every state.
#[derive(Clone)]
pub struct FaceVisibility {
    states: Vec<State>,
    faces: Vec<VoxelShape>,
    cache: HashMap<(u32, u32), bool>,
}
#[derive(Serialize, Deserialize)]
struct CachedShape {
    coords: [Vec<f64>; 3],
    occupied: Vec<bool>,
    cube: bool,
    coordinate_cubes: [bool; 3],
}
#[derive(Serialize, Deserialize)]
struct CachedState {
    block: u32,
    faces: [u32; 6],
    skip: u8,
    can_occlude: bool,
    bars_tag: bool,
    connections: u8,
    connection_properties: u8,
    fluid: String,
}
#[derive(Serialize, Deserialize)]
struct CachedVisibility {
    states: Vec<CachedState>,
    faces: Vec<CachedShape>,
}
impl FaceVisibility {
    /// Cache the immutable prepared state/face geometry. The runtime pair cache
    /// is intentionally empty after restoring it.
    pub fn to_cache_bytes(&self) -> io::Result<Vec<u8>> {
        let cached = CachedVisibility {
            states: self
                .states
                .iter()
                .map(|s| CachedState {
                    block: s.block.0,
                    faces: s.faces,
                    skip: match s.definition.skip {
                        SkipRendering::Never => 0,
                        SkipRendering::SameBlock => 1,
                        SkipRendering::Bars => 2,
                        SkipRendering::Leaves => 3,
                        SkipRendering::Liquid => 4,
                        SkipRendering::MangroveRoots => 5,
                    },
                    can_occlude: s.definition.can_occlude,
                    bars_tag: s.definition.bars_tag,
                    connections: s.connections,
                    connection_properties: s.connection_properties,
                    fluid: s.fluid.name().to_owned(),
                })
                .collect(),
            faces: self
                .faces
                .iter()
                .map(|shape| CachedShape {
                    coords: [Axis::X, Axis::Y, Axis::Z]
                        .map(|axis| shape.coordinates(axis).to_vec()),
                    occupied: shape.occupied_cells(),
                    cube: shape.is_cube(),
                    coordinate_cubes: shape.cube_coordinate_axes(),
                })
                .collect(),
        };
        serde_json::to_vec(&cached).map_err(|e| invalid(e.to_string()))
    }

    pub fn from_cache_bytes(bytes: &[u8], expected_states: usize) -> io::Result<Self> {
        let cached: CachedVisibility =
            serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
        if cached.states.len() != expected_states
            || cached.faces.len() < 2
            || cached.faces.len() > expected_states.saturating_mul(6).saturating_add(2)
        {
            return Err(invalid("invalid cached visibility dimensions"));
        }
        let mut faces = Vec::with_capacity(cached.faces.len());
        for (i, shape) in cached.faces.into_iter().enumerate() {
            let sizes = shape
                .coords
                .each_ref()
                .map(|axis| axis.len().saturating_sub(1));
            let restored = if i == 0 {
                VoxelShape::empty()
            } else if i == 1 {
                VoxelShape::block()
            } else if shape.cube {
                VoxelShape::from_cube(sizes, &shape.occupied).map_err(|e| invalid(e.to_string()))?
            } else {
                VoxelShape::from_mixed_grid(shape.coords, &shape.occupied, shape.coordinate_cubes)
                    .map_err(|e| invalid(e.to_string()))?
            };
            faces.push(restored);
        }
        let mut states = Vec::with_capacity(cached.states.len());
        for s in cached.states {
            if s.faces.iter().any(|&id| id as usize >= faces.len())
                || s.block as usize >= expected_states
            {
                return Err(invalid("invalid cached face or state ID"));
            }
            let skip = match s.skip {
                0 => SkipRendering::Never,
                1 => SkipRendering::SameBlock,
                2 => SkipRendering::Bars,
                3 => SkipRendering::Leaves,
                4 => SkipRendering::Liquid,
                5 => SkipRendering::MangroveRoots,
                _ => return Err(invalid("invalid cached render rule")),
            };
            states.push(State {
                block: BlockStateId(s.block),
                faces: s.faces,
                definition: RenderDefinition {
                    skip,
                    can_occlude: s.can_occlude,
                    bars_tag: s.bars_tag,
                },
                connections: s.connections,
                connection_properties: s.connection_properties,
                fluid: FluidKind::from_name(&s.fluid)
                    .ok_or_else(|| invalid("invalid cached fluid"))?,
            });
        }
        Ok(Self {
            states,
            faces,
            cache: HashMap::new(),
        })
    }
    pub fn new(
        occlusion: &CachedCollisionRegistry<'_>,
        definitions: &BTreeMap<String, RenderDefinition>,
    ) -> io::Result<Self> {
        let registry = occlusion.registry();
        let mut states = Vec::with_capacity(registry.len());
        // IDs 0 and 1 represent the canonical empty/full face singletons used
        // by BlockState.initCache; other empty-like shapes retain their identity.
        let mut faces = vec![VoxelShape::empty(), VoxelShape::block()];
        let mut shape_faces = HashMap::new();
        for id in 0..registry.len() {
            let id = BlockStateId(id as u32);
            let state = registry
                .state(id)
                .ok_or_else(|| invalid("unknown render state"))?;
            let definition = *definitions
                .get(&state.name)
                .ok_or_else(|| invalid("missing render definition"))?;
            let shape = occlusion
                .cached_shape(id)?
                .ok_or_else(|| invalid("missing occlusion geometry"))?;
            let face_ids = if !definition.can_occlude || shape.is_empty() {
                [0; 6]
            } else if let Some(ids) = shape_faces.get(&std::ptr::from_ref(shape)) {
                *ids
            } else {
                let mut ids = [1; 6];
                if shape
                    .join_is_not_empty(&faces[1], ShapeOp::XOR)
                    .map_err(|e| invalid(e.to_string()))?
                {
                    for direction in Direction::ALL {
                        let face = shape
                            .face_shape(direction)
                            .map_err(|e| invalid(e.to_string()))?;
                        ids[direction as usize] = if face.same_identity(&faces[0]) {
                            0
                        } else if face.same_identity(&faces[1]) {
                            1
                        } else {
                            let id = faces.len() as u32;
                            faces.push(face);
                            id
                        };
                    }
                }
                shape_faces.insert(std::ptr::from_ref(shape), ids);
                ids
            };
            let mut connections = 0;
            let mut connection_properties = 0;
            for (name, direction) in [
                ("north", Direction::North),
                ("south", Direction::South),
                ("west", Direction::West),
                ("east", Direction::East),
            ] {
                if let Some(value @ ("false" | "true")) =
                    state.properties.get(name).map(String::as_str)
                {
                    connection_properties |= 1 << direction as u8;
                    if value == "true" {
                        connections |= 1 << direction as u8;
                    }
                }
            }
            if definition.skip == SkipRendering::Bars && connection_properties != 0b111100 {
                return Err(invalid("bars rendering requires four boolean connections"));
            }
            states.push(State {
                block: registry
                    .default_state(&state.name)
                    .ok_or_else(|| invalid("missing render block identity"))?,
                faces: face_ids,
                definition,
                connections,
                connection_properties,
                fluid: FluidState::from_block(state)?.kind(),
            });
        }
        Ok(Self {
            states,
            faces,
            cache: HashMap::new(),
        })
    }
    fn state(&self, id: BlockStateId) -> io::Result<State> {
        self.states
            .get(id.0 as usize)
            .copied()
            .ok_or_else(|| invalid("unknown render state ID"))
    }
    fn skips(a: State, b: State, direction: Direction, cutout_leaves: bool) -> bool {
        let vertical = matches!(direction, Direction::Down | Direction::Up);
        let bit = 1 << direction as u8;
        let opposite = 1 << direction.opposite() as u8;
        match a.definition.skip {
            SkipRendering::Never => false,
            SkipRendering::SameBlock => a.block == b.block,
            SkipRendering::Leaves => !cutout_leaves && b.definition.skip == SkipRendering::Leaves,
            SkipRendering::Liquid => a.fluid.same_family(b.fluid),
            SkipRendering::MangroveRoots => a.block == b.block && vertical,
            SkipRendering::Bars => {
                (a.block == b.block
                    || (a.definition.bars_tag
                        && b.definition.bars_tag
                        && b.connection_properties & opposite != 0))
                    && (vertical || (a.connections & bit != 0 && b.connections & opposite != 0))
            }
        }
    }
    /// Whether the state's occlusion face toward `direction` is the full square.
    pub fn full_face(&self, state: BlockStateId, direction: Direction) -> bool {
        self.states
            .get(state.0 as usize)
            .is_some_and(|s| s.faces[direction as usize] == 1)
    }
    /// The state's occlusion face toward `direction` (`getFaceOcclusionShape`) and
    /// its shared ID: 0 is the empty singleton, 1 the full-block singleton.
    pub fn occlusion_face(
        &self,
        state: BlockStateId,
        direction: Direction,
    ) -> Option<(u32, &VoxelShape)> {
        let id = self.states.get(state.0 as usize)?.faces[direction as usize];
        Some((id, &self.faces[id as usize]))
    }
    /// Whether the state's occlusion face toward `direction` is empty.
    pub fn empty_face(&self, state: BlockStateId, direction: Direction) -> bool {
        self.states
            .get(state.0 as usize)
            .is_none_or(|s| s.faces[direction as usize] == 0)
    }
    pub fn skip_rendering(
        &self,
        state: BlockStateId,
        neighbor: BlockStateId,
        direction: Direction,
        cutout_leaves: bool,
    ) -> io::Result<bool> {
        Ok(Self::skips(
            self.state(state)?,
            self.state(neighbor)?,
            direction,
            cutout_leaves,
        ))
    }
    pub fn should_render_face(
        &mut self,
        state: BlockStateId,
        neighbor: BlockStateId,
        direction: Direction,
        cutout_leaves: bool,
    ) -> io::Result<bool> {
        let a = self.state(state)?;
        let b = self.state(neighbor)?;
        let neighbor_face = b.faces[direction.opposite() as usize];
        if neighbor_face == 1 || Self::skips(a, b, direction, cutout_leaves) {
            return Ok(false);
        }
        let own_face = a.faces[direction as usize];
        if neighbor_face == 0 || own_face == 0 {
            return Ok(true);
        }
        let key = (own_face, neighbor_face);
        if let Some(value) = self.cache.get(&key) {
            return Ok(*value);
        }
        let visible = self.faces[own_face as usize]
            .join_is_not_empty(&self.faces[neighbor_face as usize], ShapeOp::ONLY_FIRST)
            .map_err(|e| invalid(e.to_string()))?;
        // Cache policy affects speed only. Keep bounded memory while retaining
        // more shared face pairs than the native 256-entry per-thread cache.
        if self.cache.len() == 4096 {
            self.cache.clear();
        }
        self.cache.insert(key, visible);
        Ok(visible)
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn visibility_cache_preserves_partial_face_geometry() {
        let partial = VoxelShape::from_grid(
            [vec![0.0, 0.5, 1.0], vec![0.0, 1.0], vec![0.0, 1.0]],
            &[true, false],
        )
        .unwrap();
        let original = FaceVisibility {
            states: vec![State {
                block: BlockStateId(0),
                faces: [2; 6],
                definition: RenderDefinition {
                    skip: SkipRendering::SameBlock,
                    can_occlude: true,
                    bars_tag: false,
                },
                connections: 0,
                connection_properties: 0,
                fluid: FluidKind::Empty,
            }],
            faces: vec![VoxelShape::empty(), VoxelShape::block(), partial],
            cache: HashMap::new(),
        };
        let restored =
            FaceVisibility::from_cache_bytes(&original.to_cache_bytes().unwrap(), 1).unwrap();
        assert_eq!(restored.states.len(), original.states.len());
        assert_eq!(restored.faces[2].sizes(), original.faces[2].sizes());
        assert_eq!(
            restored.faces[2].occupied_cells(),
            original.faces[2].occupied_cells()
        );
        assert_eq!(restored.states[0].faces, original.states[0].faces);
        assert!(restored.cache.is_empty());
    }
}
