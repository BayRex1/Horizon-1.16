//! Versioned, server-authoritative unit-cube snapshots over the normal play
//! CustomPayload packet. Full membership supports late joins; `removed` also
//! gives explicit removal events. No client physics/asset dependency.
use crate::{
    Error, Result,
    buf::{Reader, Writer},
};
pub const CHANNEL: &str = "horizon:physics_v1";
pub const MAX_BODIES: usize = 16;
const MAX_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq)]
pub struct CubeState {
    pub id: u64,
    pub position: [f64; 3],
    /// Unit quaternion, xyzw.
    pub rotation: [f64; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub dimension: String,
    /// New world instance; monotonically allocated for one server process.
    pub epoch: u64,
    pub sequence: u64,
    /// Number of completed 1/60s physics steps (not render frames).
    pub step: u64,
    pub bodies: Vec<CubeState>,
    pub removed: Vec<u64>,
}
impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        let valid_id = |id: u64| id > 0 && id <= i64::MAX as u64;
        if self.dimension.is_empty()
            || self.dimension.len() > 128
            || !self
                .dimension
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_:/.-".contains(&b))
            || !valid_id(self.epoch)
            || !valid_id(self.sequence)
            || self.step > i64::MAX as u64
            || self.bodies.len() > MAX_BODIES
            || self.removed.len() > MAX_BODIES
        {
            return Err(Error::new("invalid physics snapshot header"));
        }
        for (i, body) in self.bodies.iter().enumerate() {
            let norm: f64 = body.rotation.iter().map(|v| v * v).sum();
            if !valid_id(body.id)
                || self.bodies[..i].iter().any(|b| b.id == body.id)
                || !body
                    .position
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 30_000_001.0)
                || !norm.is_finite()
                || (norm - 1.0).abs() > 1e-5
            {
                return Err(Error::new("invalid physics body"));
            }
        }
        for (i, &id) in self.removed.iter().enumerate() {
            if !valid_id(id)
                || self.removed[..i].contains(&id)
                || self.bodies.iter().any(|b| b.id == id)
            {
                return Err(Error::new("invalid physics removal"));
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut w = Writer::new();
        w.u8(1);
        w.string(&self.dimension);
        for value in [self.epoch, self.sequence, self.step] {
            w.i64(value as i64);
        }
        w.varint(self.bodies.len() as i32);
        for body in &self.bodies {
            w.i64(body.id as i64);
            for v in body.position.iter().chain(&body.rotation) {
                w.f64(*v);
            }
        }
        w.varint(self.removed.len() as i32);
        for &id in &self.removed {
            w.i64(id as i64);
        }
        Ok(w.data)
    }
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() > MAX_BYTES {
            return Err(Error::new("physics payload too large"));
        }
        let mut r = Reader::new(data);
        if r.u8()? != 1 {
            return Err(Error::new("unsupported physics protocol version"));
        }
        let dimension = r.string(128)?;
        let epoch = r.i64()? as u64;
        let sequence = r.i64()? as u64;
        let step = r.i64()? as u64;
        let count = r.varint()?;
        if !(0..=MAX_BODIES as i32).contains(&count) {
            return Err(Error::new("physics body count"));
        }
        let mut bodies = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let id = r.i64()? as u64;
            let position = [r.f64()?, r.f64()?, r.f64()?];
            let rotation = [r.f64()?, r.f64()?, r.f64()?, r.f64()?];
            bodies.push(CubeState {
                id,
                position,
                rotation,
            });
        }
        let count = r.varint()?;
        if !(0..=MAX_BODIES as i32).contains(&count) {
            return Err(Error::new("physics removal count"));
        }
        let mut removed = Vec::with_capacity(count as usize);
        for _ in 0..count {
            removed.push(r.i64()? as u64);
        }
        if r.remaining() != 0 {
            return Err(Error::new("trailing physics data"));
        }
        let snapshot = Self {
            dimension,
            epoch,
            sequence,
            step,
            bodies,
            removed,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> Snapshot {
        Snapshot {
            dimension: "minecraft:overworld".into(),
            epoch: 1,
            sequence: 2,
            step: 3,
            bodies: vec![CubeState {
                id: 4,
                position: [-8.5, 80.0, 8.5],
                rotation: [0.0, 0.0, 0.0, 1.0],
            }],
            removed: vec![5],
        }
    }
    #[test]
    fn round_trip_and_all_truncations() {
        let frame = frame();
        let bytes = frame.encode().unwrap();
        assert_eq!(Snapshot::decode(&bytes).unwrap(), frame);
        for n in 0..bytes.len() {
            assert!(Snapshot::decode(&bytes[..n]).is_err());
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(Snapshot::decode(&extra).is_err());
    }
    #[test]
    fn rejects_invalid_values_versions_and_duplicate_membership() {
        let mut f = frame();
        f.bodies[0].position[0] = f64::NAN;
        assert!(f.encode().is_err());
        let mut f = frame();
        f.bodies[0].rotation = [0.0; 4];
        assert!(f.encode().is_err());
        let mut f = frame();
        f.bodies.push(f.bodies[0].clone());
        assert!(f.encode().is_err());
        let mut f = frame();
        f.removed.push(4);
        assert!(f.encode().is_err());
        let mut f = frame();
        f.removed.push(5);
        assert!(f.encode().is_err());
        let mut f = frame();
        f.epoch = 0;
        assert!(f.encode().is_err());
        let mut f = frame();
        f.bodies[0].id = 0;
        assert!(f.encode().is_err());
        let mut bytes = frame().encode().unwrap();
        bytes[0] = 2;
        assert!(Snapshot::decode(&bytes).is_err());
        assert!(Snapshot::decode(&vec![0; MAX_BYTES + 1]).is_err());
    }
}
