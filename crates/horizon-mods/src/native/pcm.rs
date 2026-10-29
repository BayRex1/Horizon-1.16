//! Bounded PCM queues shared with a device worker. CPAL streams live only on
//! that worker (including on platforms where a stream is not Send).
use super::{audio, microphone};
#[cfg(feature = "audio")]
use std::sync::mpsc::{SyncSender, sync_channel};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

#[cfg_attr(not(feature = "audio"), allow(dead_code))]
struct Source {
    rate: u32,
    samples: VecDeque<f32>,
    phase: f64,
    settings: audio::Settings,
}
struct Core {
    inputs: Vec<microphone::Device>,
    outputs: Vec<audio::Device>,
    frames: VecDeque<microphone::Frame>,
    input_error: Option<String>,
    output_error: Option<String>,
    capturing: bool,
    capture_generation: u64,
    gain: f32,
    sources: HashMap<u32, Source>,
    next: u32,
    listener: [f64; 3],
    yaw: f32,
    master: f32,
}
impl Default for Core {
    fn default() -> Self {
        Self {
            inputs: vec![],
            outputs: vec![],
            frames: VecDeque::new(),
            input_error: None,
            output_error: None,
            capturing: false,
            capture_generation: 0,
            gain: 1.0,
            sources: HashMap::new(),
            next: 0,
            listener: [0.0; 3],
            yaw: 0.0,
            master: 1.0,
        }
    }
}
#[cfg(feature = "audio")]
enum Command {
    Start(String, u64),
    Stop,
    Output(String),
}
#[derive(Default)]
pub struct Pcm {
    core: Arc<Mutex<Core>>,
    #[cfg(feature = "audio")]
    worker: Option<SyncSender<Command>>,
    output_started: bool,
}
fn gain_ok(gain: f32) -> Result<(), String> {
    if gain.is_finite() && (0.0..=4.0).contains(&gain) {
        Ok(())
    } else {
        Err("gain must be finite, in 0..4".into())
    }
}
impl Pcm {
    fn ensure_worker(&mut self) -> Result<(), String> {
        #[cfg(feature = "audio")]
        {
            if std::env::var_os("HORIZON_NO_AUDIO").is_some() {
                return Err("audio disabled by HORIZON_NO_AUDIO".into());
            }
            if self.worker.is_none() {
                let (tx, rx) = sync_channel(8);
                let core = self.core.clone();
                std::thread::Builder::new()
                    .name("mod audio".into())
                    .spawn(move || device_worker(core, rx))
                    .map_err(|e| e.to_string())?;
                self.worker = Some(tx);
            }
            Ok(())
        }
        #[cfg(not(feature = "audio"))]
        {
            Err("this Horizon was built without audio".into())
        }
    }
    pub fn is_capturing(&self) -> bool {
        self.core.lock().unwrap().capturing
    }
    pub fn master_gain(&mut self, gain: f32) {
        self.core.lock().unwrap().master = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    pub fn input_devices(&mut self) -> Vec<microphone::Device> {
        let _ = self.ensure_worker();
        self.core.lock().unwrap().inputs.clone()
    }
    pub fn output_devices(&mut self) -> Vec<audio::Device> {
        let _ = self.ensure_worker();
        self.core.lock().unwrap().outputs.clone()
    }
    pub fn start(&mut self, device: String, gain: f32) -> Result<(), String> {
        gain_ok(gain)?;
        if device.len() > 128 {
            return Err("device id too long".into());
        }
        self.ensure_worker()?;
        let generation = {
            let mut c = self.core.lock().unwrap();
            c.frames.clear();
            c.input_error = None;
            c.gain = gain;
            c.capturing = true;
            c.capture_generation = c.capture_generation.wrapping_add(1);
            c.capture_generation
        };
        #[cfg(feature = "audio")]
        if let Err(e) = self
            .worker
            .as_ref()
            .unwrap()
            .try_send(Command::Start(device, generation))
        {
            self.core.lock().unwrap().capturing = false;
            return Err(e.to_string());
        }
        #[cfg(not(feature = "audio"))]
        let _ = (device, generation);
        Ok(())
    }
    pub fn stop(&mut self) {
        let mut c = self.core.lock().unwrap();
        c.capturing = false;
        c.capture_generation = c.capture_generation.wrapping_add(1);
        c.frames.clear();
        #[cfg(feature = "audio")]
        if let Some(worker) = &self.worker {
            let _ = worker.try_send(Command::Stop);
        }
    }
    pub fn set_gain(&mut self, gain: f32) -> Result<(), String> {
        gain_ok(gain)?;
        self.core.lock().unwrap().gain = gain;
        Ok(())
    }
    pub fn read(&mut self) -> Result<Vec<microphone::Frame>, String> {
        let mut c = self.core.lock().unwrap();
        if let Some(error) = c.input_error.take() {
            return Err(error);
        }
        Ok(c.frames.drain(..).collect())
    }
    pub fn select_device(&mut self, device: String) -> Result<(), String> {
        if device.len() > 128 {
            return Err("device id too long".into());
        }
        self.ensure_worker()?;
        #[cfg(feature = "audio")]
        self.worker
            .as_ref()
            .unwrap()
            .try_send(Command::Output(device))
            .map_err(|e| e.to_string())?;
        #[cfg(not(feature = "audio"))]
        let _ = device;
        self.output_started = true;
        self.clear_sources();
        Ok(())
    }
    pub fn create(&mut self, rate: u32) -> Result<u32, String> {
        if !(8000..=96000).contains(&rate) {
            return Err("sample rate must be 8000..96000 Hz".into());
        }
        self.ensure_worker()?;
        if !self.output_started {
            #[cfg(feature = "audio")]
            self.worker
                .as_ref()
                .unwrap()
                .try_send(Command::Output(String::new()))
                .map_err(|e| e.to_string())?;
            self.output_started = true;
        }
        let mut c = self.core.lock().unwrap();
        if c.sources.len() >= 32 {
            return Err("audio source limit reached".into());
        }
        c.next = c.next.checked_add(1).ok_or("audio handle limit reached")?;
        let id = c.next;
        c.sources.insert(
            id,
            Source {
                rate,
                samples: VecDeque::new(),
                phase: 0.0,
                settings: audio::Settings {
                    gain: 1.0,
                    at: None,
                    distance: 48.0,
                },
            },
        );
        Ok(id)
    }
    pub fn write(&mut self, source: u32, samples: Vec<f32>) -> Result<(), String> {
        let mut c = self.core.lock().unwrap();
        if let Some(error) = c.output_error.take() {
            return Err(error);
        }
        let s = c.sources.get_mut(&source).ok_or("unknown audio source")?;
        if s.samples.len() + samples.len() > s.rate as usize / 5 {
            return Err("audio buffer full (200 ms)".into());
        }
        if samples.iter().any(|v| !v.is_finite() || v.abs() > 1.0) {
            return Err("PCM must be finite and normalized to [-1, 1]".into());
        }
        s.samples.extend(samples);
        Ok(())
    }
    pub fn configure(&mut self, source: u32, settings: audio::Settings) -> Result<(), String> {
        gain_ok(settings.gain)?;
        if !settings.distance.is_finite()
            || settings.distance <= 0.0
            || settings
                .at
                .as_ref()
                .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite())
        {
            return Err("invalid audio position or distance".into());
        }
        self.core
            .lock()
            .unwrap()
            .sources
            .get_mut(&source)
            .ok_or("unknown audio source")?
            .settings = settings;
        Ok(())
    }
    pub fn close(&mut self, source: u32) {
        self.core.lock().unwrap().sources.remove(&source);
    }
    pub fn clear_sources(&mut self) {
        self.core.lock().unwrap().sources.clear();
    }
    pub fn listener(&mut self, position: [f64; 3], yaw: f32) {
        let mut c = self.core.lock().unwrap();
        c.listener = position;
        c.yaw = yaw;
    }
}
impl Drop for Pcm {
    fn drop(&mut self) {
        // Silence callbacks immediately. Dropping the sole sender closes streams
        // on their owning worker, without waiting on a device on the game thread.
        let mut c = self.core.lock().unwrap();
        c.capturing = false;
        c.capture_generation = c.capture_generation.wrapping_add(1);
        c.frames.clear();
        c.sources.clear();
    }
}

#[cfg(any(feature = "audio", test))]
fn capture(c: &mut Core, rate: u32, samples: Vec<f32>) {
    if !c.capturing {
        return;
    }
    if c.frames.len() == 10 {
        c.frames.pop_front();
    }
    let samples = samples
        .into_iter()
        .map(|s| {
            if s.is_finite() {
                (s * c.gain).clamp(-1.0, 1.0)
            } else {
                0.0
            }
        })
        .collect();
    c.frames.push_back(microphone::Frame {
        sample_rate: rate,
        samples,
    });
}
#[cfg(any(feature = "audio", test))]
fn stereo_gains(settings: &audio::Settings, listener: [f64; 3], yaw: f32) -> [f32; 2] {
    let Some(p) = &settings.at else {
        return [settings.gain; 2];
    };
    let d = [p.x - listener[0], p.y - listener[1], p.z - listener[2]];
    let distance = d.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !distance.is_finite() || distance >= f64::from(settings.distance) {
        return [0.0; 2];
    }
    let gain =
        settings.gain * (1.0 - distance / f64::from(settings.distance)).clamp(0.0, 1.0) as f32;
    let yaw = f64::from(yaw).to_radians();
    let pan = if distance > 0.0 {
        ((-d[0] * yaw.cos() - d[2] * yaw.sin()) / distance).clamp(-1.0, 1.0) as f32
    } else {
        0.0
    };
    [
        gain * (1.0 - pan).sqrt() * std::f32::consts::FRAC_1_SQRT_2,
        gain * (1.0 + pan).sqrt() * std::f32::consts::FRAC_1_SQRT_2,
    ]
}
#[cfg(any(feature = "audio", test))]
fn mix(c: &mut Core, rate: u32, channels: usize, out: &mut [f32]) {
    out.fill(0.0);
    for source in c.sources.values_mut() {
        let gains = stereo_gains(&source.settings, c.listener, c.yaw);
        let step = f64::from(source.rate) / f64::from(rate);
        for frame in out.chunks_exact_mut(channels) {
            let Some(&first) = source.samples.front() else {
                source.phase = 0.0;
                break;
            };
            let second = source.samples.get(1).copied().unwrap_or(first);
            let sample = first + (second - first) * source.phase as f32;
            if channels == 1 {
                frame[0] += sample * (gains[0] + gains[1]) * 0.5;
            } else {
                frame[0] += sample * gains[0];
                frame[1] += sample * gains[1];
            }
            source.phase += step;
            let consumed = source.phase.floor() as usize;
            for _ in 0..consumed.min(source.samples.len()) {
                source.samples.pop_front();
            }
            source.phase -= consumed as f64;
        }
    }
    for sample in out {
        *sample = (*sample * c.master).clamp(-1.0, 1.0);
    }
}

#[cfg(feature = "audio")]
fn device_worker(core: Arc<Mutex<Core>>, rx: std::sync::mpsc::Receiver<Command>) {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let inputs: Vec<_> = host
        .input_devices()
        .map(|ds| ds.take(64).collect())
        .unwrap_or_default();
    let outputs: Vec<_> = host
        .output_devices()
        .map(|ds| ds.take(64).collect())
        .unwrap_or_default();
    let input_names = inputs
        .iter()
        .enumerate()
        .map(|(i, d)| microphone::Device {
            id: i.to_string(),
            name: d.name().unwrap_or_else(|_| "Microphone".into()),
        })
        .collect();
    let output_names = outputs
        .iter()
        .enumerate()
        .map(|(i, d)| audio::Device {
            id: i.to_string(),
            name: d.name().unwrap_or_else(|_| "Speaker".into()),
        })
        .collect();
    {
        let mut c = core.lock().unwrap();
        c.inputs = input_names;
        c.outputs = output_names;
    }
    let mut input = None;
    let mut output = None;

    while let Ok(command) = rx.recv() {
        if !core.lock().unwrap().capturing {
            input = None;
        }
        match command {
            Command::Start(id, generation) => {
                {
                    let c = core.lock().unwrap();
                    if !c.capturing || c.capture_generation != generation {
                        continue;
                    }
                }
                input = None;
                let device = if id.is_empty() {
                    host.default_input_device()
                } else {
                    id.parse::<usize>()
                        .ok()
                        .and_then(|i| inputs.get(i).cloned())
                };
                match device
                    .ok_or_else(|| "microphone not found".into())
                    .and_then(|d| open_input(&d, core.clone(), generation))
                {
                    Ok(stream) => {
                        let c = core.lock().unwrap();
                        if c.capturing && c.capture_generation == generation {
                            input = Some(stream);
                        }
                    }
                    Err(e) => {
                        let mut c = core.lock().unwrap();
                        if c.capture_generation == generation {
                            c.input_error = Some(e);
                            c.capturing = false;
                        }
                    }
                }
            }
            Command::Stop => {
                input = None;
            }
            Command::Output(id) => {
                output = None;
                let device = if id.is_empty() {
                    host.default_output_device()
                } else {
                    id.parse::<usize>()
                        .ok()
                        .and_then(|i| outputs.get(i).cloned())
                };
                match device
                    .ok_or_else(|| "speaker not found".into())
                    .and_then(|d| open_output(&d, core.clone()))
                {
                    Ok(stream) => {
                        core.lock().unwrap().output_error = None;
                        output = Some(stream);
                    }
                    Err(e) => core.lock().unwrap().output_error = Some(e),
                }
            }
        }
        // Streams are owned and dropped on this thread. No input is opened by
        // merely enumerating or playing audio.
        if !core.lock().unwrap().capturing {
            input = None;
        }
        let _ = (&input, &output);
    }
}

#[cfg(feature = "audio")]
fn open_input(
    device: &cpal::Device,
    core: Arc<Mutex<Core>>,
    generation: u64,
) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0;
    if !(8000..=192000).contains(&rate) || config.channels == 0 {
        return Err("unsupported microphone configuration".into());
    }
    let channels = usize::from(config.channels);
    let size = rate as usize / 50;
    let errors = core.clone();
    let error = move |e: cpal::StreamError| {
        let mut c = errors.lock().unwrap();
        if c.capture_generation == generation {
            c.input_error = Some(e.to_string());
            c.capturing = false;
        }
    };
    macro_rules! build {
        ($ty:ty) => {{
            let mut pending = Vec::with_capacity(size);
            device.build_input_stream(
                &config,
                move |data: &[$ty], _| {
                    use cpal::Sample;
                    let Ok(mut c) = core.try_lock() else {
                        pending.clear();
                        return;
                    };
                    if !c.capturing || c.capture_generation != generation {
                        pending.clear();
                        return;
                    }
                    for frame in data.chunks_exact(channels) {
                        pending.push(
                            frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>()
                                / channels as f32,
                        );
                        if pending.len() == size {
                            capture(
                                &mut c,
                                rate,
                                std::mem::replace(&mut pending, Vec::with_capacity(size)),
                            );
                        }
                    }
                },
                error,
                None,
            )
        }};
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::F64 => build!(f64),
        cpal::SampleFormat::U16 => build!(u16),
        _ => return Err("unsupported microphone PCM format".into()),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}
#[cfg(feature = "audio")]
fn open_output(device: &cpal::Device, core: Arc<Mutex<Core>>) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, StreamTrait};
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate.0;
    let channels = usize::from(config.channels);
    if channels == 0 || rate == 0 {
        return Err("unsupported output configuration".into());
    }
    let errors = core.clone();
    let error = move |e: cpal::StreamError| {
        errors.lock().unwrap().output_error = Some(e.to_string());
    };
    macro_rules! build {
        ($ty:ty) => {{
            let mut scratch = Vec::new();
            device.build_output_stream(
                &config,
                move |data: &mut [$ty], _| {
                    use cpal::Sample;
                    scratch.resize(data.len(), 0.0);
                    if let Ok(mut c) = core.try_lock() {
                        mix(&mut c, rate, channels, &mut scratch);
                    } else {
                        scratch.fill(0.0);
                    }
                    for (out, sample) in data.iter_mut().zip(&scratch) {
                        *out = sample.to_sample::<$ty>();
                    }
                },
                error,
                None,
            )
        }};
    }
    let stream = match format {
        cpal::SampleFormat::F32 => build!(f32),
        cpal::SampleFormat::I16 => build!(i16),
        cpal::SampleFormat::I32 => build!(i32),
        cpal::SampleFormat::F64 => build!(f64),
        cpal::SampleFormat::U16 => build!(u16),
        _ => return Err("unsupported speaker PCM format".into()),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_is_bounded_muted_and_clipped() {
        let mut c = Core {
            capturing: true,
            gain: 4.0,
            ..Core::default()
        };
        for _ in 0..20 {
            capture(&mut c, 48000, vec![0.5, -0.5, f32::NAN]);
        }
        assert_eq!(c.frames.len(), 10);
        assert_eq!(c.frames[0].samples, [1.0, -1.0, 0.0]);
        c.capturing = false;
        capture(&mut c, 48000, vec![1.0]);
        assert_eq!(c.frames.len(), 10);
    }
    #[test]
    fn output_validation_is_atomic_and_sources_are_isolated() {
        let mut pcm = Pcm::default();
        pcm.core.lock().unwrap().sources.insert(
            1,
            Source {
                rate: 8000,
                samples: VecDeque::new(),
                phase: 0.0,
                settings: audio::Settings {
                    gain: 1.0,
                    at: None,
                    distance: 10.0,
                },
            },
        );
        assert!(pcm.write(2, vec![0.1]).is_err());
        assert!(pcm.write(1, vec![0.0, f32::NAN]).is_err());
        assert!(pcm.core.lock().unwrap().sources[&1].samples.is_empty());
        assert!(pcm.write(1, vec![0.25; 1600]).is_ok());
        assert!(pcm.write(1, vec![0.25]).is_err());
        assert_eq!(pcm.core.lock().unwrap().sources[&1].samples.len(), 1600);
        assert!(
            pcm.configure(
                1,
                audio::Settings {
                    gain: f32::INFINITY,
                    at: None,
                    distance: 1.0
                }
            )
            .is_err()
        );
        pcm.close(1);
        assert!(pcm.core.lock().unwrap().sources.is_empty());
    }
    #[test]
    fn mix_resamples_mixes_and_attenuates() {
        let mut c = Core::default();
        c.sources.insert(
            1,
            Source {
                rate: 8000,
                samples: VecDeque::from(vec![1.0; 8]),
                phase: 0.0,
                settings: audio::Settings {
                    gain: 0.5,
                    at: None,
                    distance: 10.0,
                },
            },
        );
        let mut out = [0.0; 32];
        mix(&mut c, 16000, 2, &mut out);
        assert!(out.iter().all(|&s| s == 0.5));
        assert!(c.sources[&1].samples.is_empty());
        c.sources.get_mut(&1).unwrap().samples = VecDeque::from(vec![1.0; 8]);
        c.master = 0.25;
        mix(&mut c, 16000, 2, &mut out);
        assert!(out.iter().all(|&s| s == 0.125));
        mix(&mut c, 16000, 2, &mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        let settings = audio::Settings {
            gain: 1.0,
            at: Some(audio::Position {
                x: -5.0,
                y: 0.0,
                z: 0.0,
            }),
            distance: 10.0,
        };
        let gains = stereo_gains(&settings, [0.0; 3], 0.0);
        assert_eq!(gains[0], 0.0);
        assert!((gains[1] - 0.5).abs() < 0.0001);
        assert_eq!(stereo_gains(&settings, [100.0; 3], 0.0), [0.0; 2]);
    }
}
