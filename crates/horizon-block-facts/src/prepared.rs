//! Prepared facts are an optional, versioned acceleration of the authoritative JSON.
//! A modified resource root (including one synthesized for mods) uses the JSON loader.
use super::{BlockFacts, StateFacts};
use crate::Result;
use horizon_world::{DATA_VERSION, nbt::Tag, registry::BlockRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, io::Read, path::Path, sync::Arc, time::UNIX_EPOCH};

const FILE: &str = "gameplay-facts-prepared.bin";
const VERSION: u32 = 2;
const MAGIC: &[u8; 4] = b"HBF2";
const SOURCES: [&str; 3] = ["facts.json", "gameplay-facts.json", "block-behaviours.json"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    len: u64,
    modified_ns: u64,
}

fn stamps(root: &Path) -> Result<[Stamp; 3]> {
    let mut result = [Stamp {
        len: 0,
        modified_ns: 0,
    }; 3];
    for (i, source) in SOURCES.iter().enumerate() {
        let meta = fs::metadata(root.join(source))?;
        let ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_nanos() as u64);
        result[i] = Stamp {
            len: meta.len(),
            modified_ns: ns,
        };
    }
    Ok(result)
}

#[derive(Serialize, Deserialize)]
struct Prepared {
    version: u32,
    stamps: [Stamp; 3],
    source_hashes: [[u8; 32]; 3],
    air: u32,
    // The shape indices preserve the original sharing partition, including
    // distinct fallback shapes with equal geometry.
    shapes: Vec<Vec<[f64; 6]>>,
    shape_ids: Vec<[u32; 2]>,
    states: Vec<StateFacts>,
}

fn source_stamps(root: &Path) -> Option<[Stamp; 3]> {
    if SOURCES.iter().any(|source| !root.join(source).is_file()) {
        return None;
    }
    stamps(root).ok()
}

fn source_hashes(root: &Path) -> Result<[[u8; 32]; 3]> {
    let mut hashes = [[0; 32]; 3];
    for (index, source) in SOURCES.iter().enumerate() {
        let mut file = fs::File::open(root.join(source))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0; 1 << 16];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        hashes[index] = hasher.finalize().into();
    }
    Ok(hashes)
}

fn decode(bytes: &[u8]) -> Option<Prepared> {
    let body = bytes.strip_prefix(MAGIC)?;
    let (checksum, payload) = body.split_at_checked(32)?;
    if Sha256::digest(payload).as_slice() != checksum {
        return None;
    }
    postcard::from_bytes(payload).ok()
}

fn encode(prepared: &Prepared) -> Result<Vec<u8>> {
    let payload = postcard::to_allocvec(prepared)?;
    let mut bytes = Vec::with_capacity(MAGIC.len() + 32 + payload.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&Sha256::digest(&payload));
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

pub(super) fn load(root: &Path, registry: &BlockRegistry) -> Result<Option<BlockFacts>> {
    let Some(stamps) = source_stamps(root) else {
        return Ok(None);
    };
    let Ok(bytes) = fs::read(root.join(FILE)) else {
        return Ok(None);
    };
    let Some(mut saved) = decode(&bytes) else {
        return Ok(None);
    };
    if saved.version != VERSION
        || saved.stamps != stamps
        || saved.source_hashes != source_hashes(root)?
        || saved.states.len() != registry.len()
        || saved.shape_ids.len() != registry.len()
        || saved.air as usize >= registry.len()
    {
        return Ok(None);
    }
    // Even when a prepared file is copied to a modded resource directory,
    // state order and names must still match the active registry.
    for (i, state) in saved.states.iter().enumerate() {
        if registry
            .state(horizon_world::registry::BlockStateId(i as u32))
            .is_none_or(|current| current.name != state.name.as_ref())
        {
            return Ok(None);
        }
    }
    let shapes: Vec<Arc<[[f64; 6]]>> = saved
        .shapes
        .drain(..)
        .map(|shape| Arc::from(shape.into_boxed_slice()))
        .collect();
    for (state, [outline, edges]) in saved.states.iter_mut().zip(&saved.shape_ids) {
        let Some(a) = shapes.get(*outline as usize) else {
            return Ok(None);
        };
        let Some(b) = shapes.get(*edges as usize) else {
            return Ok(None);
        };
        state.outline = a.clone();
        state.outline_edges = b.clone();
    }
    Ok(Some(BlockFacts {
        states: saved.states,
        air: horizon_world::registry::BlockStateId(saved.air),
    }))
}

fn tag(value: &Value) -> Result<Tag> {
    Ok(match value {
        Value::String(s) => Tag::String(s.as_str().into()),
        Value::Number(n) => Tag::Int(i32::try_from(n.as_i64().ok_or("registry number")?)?),
        Value::Array(a) => Tag::List(a.iter().map(tag).collect::<Result<_>>()?),
        Value::Object(o) => Tag::Compound(
            o.iter()
                .map(|(k, v)| Ok((k.as_str().into(), tag(v)?)))
                .collect::<Result<_>>()?,
        ),
        _ => return Err("registry value".into()),
    })
}

fn registry(root: &Path) -> Result<BlockRegistry> {
    let facts: Value = serde_json::from_slice(&fs::read(root.join("facts.json"))?)?;
    Ok(BlockRegistry::from_nbt(&Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        (
            "states",
            Tag::List(
                facts["states"]
                    .as_array()
                    .ok_or("registry states")?
                    .iter()
                    .map(|s| tag(&s[4]))
                    .collect::<Result<_>>()?,
            ),
        ),
        ("defaults", tag(&facts["defaults"])?),
    ]))?)
}

pub(super) fn prepare(root: &Path) -> Result<()> {
    // The launcher can call resource preparation on every launch. Do not
    // reconstruct the 35k-state registry just to find an unchanged cache.
    if let Some(current) = source_stamps(root) {
        if let Ok(bytes) = fs::read(root.join(FILE)) {
            if let Some(saved) = decode(&bytes) {
                if saved.version == VERSION
                    && saved.stamps == current
                    && saved.source_hashes == source_hashes(root)?
                {
                    return Ok(());
                }
            }
        }
    }
    let active = registry(root)?;
    let facts = BlockFacts::load_json(root, &active)?;
    let mut shape_keys: HashMap<(usize, usize), u32> = HashMap::new();
    let mut shapes = Vec::new();
    let mut shape_ids = Vec::with_capacity(facts.states.len());
    let mut states = facts.states;
    for state in &mut states {
        let mut ids = [0; 2];
        for (slot, shape) in [&state.outline, &state.outline_edges]
            .into_iter()
            .enumerate()
        {
            let key = (shape.as_ptr() as usize, shape.len());
            ids[slot] = *shape_keys.entry(key).or_insert_with(|| {
                let id = shapes.len() as u32;
                shapes.push(shape.to_vec());
                id
            });
        }
        shape_ids.push(ids);
        state.outline = Arc::from([]);
        state.outline_edges = Arc::from([]);
    }
    let prepared = Prepared {
        version: VERSION,
        stamps: stamps(root)?,
        source_hashes: source_hashes(root)?,
        air: facts.air.0,
        shapes,
        shape_ids,
        states,
    };
    let data = encode(&prepared)?;
    let temporary = root.join(format!("{FILE}.tmp"));
    fs::write(&temporary, data)?;
    let destination = root.join(FILE);
    if let Err(error) = fs::rename(&temporary, &destination) {
        // On Windows rename cannot replace an existing file.
        if !destination.exists() {
            return Err(error.into());
        }
        fs::remove_file(&destination)?;
        fs::rename(&temporary, &destination)?;
    }
    Ok(())
}
