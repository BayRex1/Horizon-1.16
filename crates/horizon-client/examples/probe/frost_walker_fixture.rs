//! Frost Walker's disk on synthetic worlds for differential tests against vanilla
//! (tools/test_frost_walker_parity.py): reads one case per line and prints the
//! cells that `frost::replace_disk` sets, in order, in the oracle's format.
//!
//! A line is `LEVEL POSITION DEFAULT CELLS ENTITIES`: POSITION is the player's
//! x,y,z as f64 bit patterns in hex; DEFAULT is the kind of every block not
//! listed in CELLS (`x,y,z,kind;...` or `-`); ENTITIES are blocking boxes
//! `minx,miny,minz,maxx,maxy,maxz` (f64 bits in hex, `;`-separated or `-`).
//!
//! usage: frost_walker_fixture INPUT OUTPUT
use horizon_client::frost::{self, FrostLevel};
use horizon_core::geometry::{Aabb, Vec3, VoxelShape};
use horizon_world::entity_obstruction::{
    EntityIdentity, EntityObstructionWorld, ObstructionEntity, is_unobstructed,
};
use std::{collections::HashMap, convert::Infallible, fmt::Write as _, fs, path::Path};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Air,
    CaveAir,
    VoidAir,
    WaterSource,
    FlowingWater,
    Stone,
    Ice,
}

impl Kind {
    fn parse(name: &str) -> Result<Self, String> {
        Ok(match name {
            "air" => Self::Air,
            "cave_air" => Self::CaveAir,
            "void_air" => Self::VoidAir,
            "water" => Self::WaterSource,
            "flowing" => Self::FlowingWater,
            "stone" => Self::Stone,
            "ice" => Self::Ice,
            other => return Err(format!("unknown kind {other}")),
        })
    }
}

/// The synthetic world: every cell is `default` unless listed.
struct World {
    default: Kind,
    cells: HashMap<[i32; 3], Kind>,
    boxes: Vec<Aabb>,
}

impl World {
    fn kind(&self, at: [i32; 3]) -> Kind {
        self.cells.get(&at).copied().unwrap_or(self.default)
    }
}

/// The entities that block building, as the level's candidate snapshot.
struct Blockers<'a>(&'a [Aabb]);

impl EntityObstructionWorld for Blockers<'_> {
    type Error = Infallible;
    fn obstruction_candidates(
        &mut self,
        _excluded: Option<EntityIdentity>,
        _bounds: Aabb,
    ) -> Result<Vec<ObstructionEntity>, Infallible> {
        Ok(self
            .0
            .iter()
            .enumerate()
            .map(|(i, bounds)| ObstructionEntity {
                identity: EntityIdentity(i as u64),
                root_vehicle: EntityIdentity(i as u64),
                bounds: *bounds,
                removed: false,
                spectator: false,
                blocks_building: true,
            })
            .collect())
    }
}

fn block_box(at: [i32; 3]) -> Aabb {
    let [x, y, z] = at.map(f64::from);
    Aabb::new(Vec3::new(x, y, z), Vec3::new(x + 1.0, y + 1.0, z + 1.0))
}

impl FrostLevel for World {
    fn is_water_source(&self, at: [i32; 3]) -> bool {
        self.kind(at) == Kind::WaterSource
    }
    fn is_air(&self, at: [i32; 3]) -> bool {
        matches!(self.kind(at), Kind::Air | Kind::CaveAir | Kind::VoidAir)
    }
    fn is_unobstructed(&self, at: [i32; 3]) -> bool {
        let shape = VoxelShape::from_box(block_box(at)).expect("finite block");
        is_unobstructed(&mut Blockers(&self.boxes), None, &shape).unwrap_or(false)
    }
    fn set_frosted_ice(&mut self, at: [i32; 3]) -> bool {
        let changed = self.kind(at) != Kind::Ice;
        self.cells.insert(at, Kind::Ice);
        changed
    }
}

fn bits(token: &str) -> Result<f64, String> {
    u64::from_str_radix(token, 16)
        .map(f64::from_bits)
        .map_err(|e| format!("bad bits {token}: {e}"))
}

fn parse_line(line: &str) -> Result<(u32, Vec3, World), String> {
    let f: Vec<&str> = line.split(' ').collect();
    if f.len() != 5 {
        return Err(format!("expected 5 fields: {line}"));
    }
    let level: u32 = f[0].parse().map_err(|e| format!("level: {e}"))?;
    let p: Vec<f64> = f[1].split(',').map(bits).collect::<Result<_, _>>()?;
    let [px, py, pz] = p.as_slice() else {
        return Err("position needs 3 values".into());
    };
    let mut cells = HashMap::new();
    if f[3] != "-" {
        for cell in f[3].split(';') {
            let c: Vec<&str> = cell.split(',').collect();
            let [x, y, z, kind] = c.as_slice() else {
                return Err(format!("bad cell {cell}"));
            };
            let at = [x, y, z].map(|v| v.parse::<i32>().map_err(|e| format!("cell {cell}: {e}")));
            cells.insert(
                [at[0].clone()?, at[1].clone()?, at[2].clone()?],
                Kind::parse(kind)?,
            );
        }
    }
    let mut boxes = Vec::new();
    if f[4] != "-" {
        for entry in f[4].split(';') {
            let b: Vec<f64> = entry.split(',').map(bits).collect::<Result<_, _>>()?;
            let [a, b0, c, d, e, g] = b.as_slice() else {
                return Err(format!("bad box {entry}"));
            };
            boxes.push(Aabb::new(Vec3::new(*a, *b0, *c), Vec3::new(*d, *e, *g)));
        }
    }
    let world = World {
        default: Kind::parse(f[2])?,
        cells,
        boxes,
    };
    Ok((level, Vec3::new(*px, *py, *pz), world))
}

pub fn main() -> horizon_client::Result<()> {
    let args: Vec<String> = crate::args().collect();
    let input = fs::read_to_string(
        args.get(1)
            .ok_or("usage: frost_walker_fixture INPUT OUTPUT")?,
    )?;
    let output = args.get(2).ok_or("missing OUTPUT")?;
    let mut out = String::new();
    for (index, line) in input.lines().enumerate() {
        let (level, position, mut world) = parse_line(line)?;
        let radius = frost::radius(level);
        let set = frost::replace_disk(&mut world, position, radius);
        let cells = if set.is_empty() {
            "-".to_owned()
        } else {
            set.iter()
                .map(|[x, y, z]| format!("{x},{y},{z},{}", frost::TRIGGER))
                .collect::<Vec<_>>()
                .join(";")
        };
        writeln!(out, "{index} {cells}")?;
    }
    fs::write(Path::new(output), out)?;
    Ok(())
}
