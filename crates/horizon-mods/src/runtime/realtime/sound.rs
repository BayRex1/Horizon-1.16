//! Native streams live on a worker: platform audio handles never enter a Wasm store.
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
struct Mixer {
    sources: BTreeMap<u32, Source>,
    next: u32,
    capture: VecDeque<f32>,
    capture_limit: usize,
    listener: [f64; 3],
    yaw: f32,
    master: f32,
}
#[cfg_attr(not(feature = "native-audio"), allow(dead_code))]
struct Source {
    rate: u32,
    channels: u16,
    pcm: VecDeque<f32>,
    fraction: f64,
    gain: f32,
    at: Option<[f64; 3]>,
    radius: f32,
}
impl Default for Mixer {
    fn default() -> Self {
        Self {
            sources: BTreeMap::new(),
            next: 0,
            capture: VecDeque::new(),
            capture_limit: 0,
            listener: [0.0; 3],
            yaw: 0.0,
            master: 1.0,
        }
    }
}
impl Mixer {
    #[cfg(any(feature = "native-audio", test))]
    fn frame(&mut self, rate: u32) -> [f32; 2] {
        let mut out = [0.0; 2];
        for s in self.sources.values_mut() {
            if s.pcm.len() < s.channels as usize {
                continue;
            }
            let left = s.pcm[0];
            let right = if s.channels == 2 { s.pcm[1] } else { left };
            let mut gain = s.gain;
            let mut pan = 0.0;
            if let Some(at) = s.at {
                let delta = [
                    at[0] - self.listener[0],
                    at[1] - self.listener[1],
                    at[2] - self.listener[2],
                ];
                let distance = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
                gain *= (1.0 - distance / s.radius as f64).clamp(0.0, 1.0) as f32;
                if distance > 0.001 {
                    let yaw = self.yaw.to_radians() as f64;
                    pan = (-(delta[0] * yaw.cos() + delta[2] * yaw.sin()) / distance)
                        .clamp(-1.0, 1.0) as f32;
                }
                let mono = (left + right) * 0.5;
                out[0] += mono * gain * ((1.0 - pan) * 0.5).sqrt();
                out[1] += mono * gain * ((1.0 + pan) * 0.5).sqrt();
            } else {
                out[0] += left * gain;
                out[1] += right * gain;
            }
            s.fraction += s.rate as f64 / rate as f64;
            let consume = (s.fraction as usize).min(s.pcm.len() / s.channels as usize);
            s.fraction -= s.fraction.floor();
            s.pcm.drain(..consume * s.channels as usize);
        }
        out.map(|v| (v * self.master).clamp(-1.0, 1.0))
    }
}
#[derive(Default)]
pub(super) struct Sound {
    mixer: Arc<Mutex<Mixer>>,
    #[cfg(feature = "native-audio")]
    worker: Option<std::sync::mpsc::SyncSender<Command>>,
    #[cfg(feature = "native-audio")]
    devices: Pending<Vec<(String, String)>>,
    #[cfg(feature = "native-audio")]
    start: Pending<(u32, u16)>,
    #[cfg(feature = "native-audio")]
    starting_device: Option<String>,
    #[cfg(feature = "native-audio")]
    output: Pending<()>,
    #[cfg(feature = "native-audio")]
    output_ready: bool,
}
impl Sound {
    pub fn master(&mut self, gain: f32) {
        self.mixer.lock().unwrap().master = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    pub fn listener(&mut self, at: [f64; 3], yaw: f32) {
        let mut m = self.mixer.lock().unwrap();
        m.listener = at;
        m.yaw = yaw;
    }
    pub fn suspend(&mut self) {
        self.stop();
        self.mixer.lock().unwrap().sources.clear();
    }
    pub fn read(&mut self) -> Vec<f32> {
        let mut m = self.mixer.lock().unwrap();
        let n = m.capture.len().min(8192);
        m.capture.drain(..n).collect()
    }
    pub fn stop(&mut self) {
        // Clearing capture_limit also makes any in-flight callback discard its data.
        let mut m = self.mixer.lock().unwrap();
        m.capture.clear();
        m.capture_limit = 0;
        drop(m);
        #[cfg(feature = "native-audio")]
        {
            self.start = None;
            if let Some(tx) = &self.worker {
                let _ = tx.try_send(Command::Stop);
            }
        }
    }
    pub fn create(&mut self, rate: u32, channels: u16) -> Result<u32, String> {
        if !(8000..=96000).contains(&rate) || !(1..=2).contains(&channels) {
            return Err("expected 8–96 kHz mono/stereo PCM".into());
        }
        if self.mixer.lock().unwrap().sources.len() >= 16 {
            return Err("source limit reached".into());
        }
        self.output()?;
        let mut m = self.mixer.lock().unwrap();
        m.next = m.next.checked_add(1).ok_or("handle limit reached")?;
        let id = m.next;
        m.sources.insert(
            id,
            Source {
                rate,
                channels,
                pcm: VecDeque::new(),
                fraction: 0.0,
                gain: 1.0,
                at: None,
                radius: 32.0,
            },
        );
        Ok(id)
    }
    pub fn push(&mut self, id: u32, pcm: Vec<f32>) -> Result<(), String> {
        let mut m = self.mixer.lock().unwrap();
        let s = m.sources.get_mut(&id).ok_or("unknown source")?;
        if pcm.len() > 8192
            || !pcm.len().is_multiple_of(s.channels as usize)
            || pcm.iter().any(|s| !s.is_finite() || s.abs() > 1.0)
        {
            return Err("invalid PCM buffer".into());
        }
        if s.pcm.len() + pcm.len() > s.rate as usize * s.channels as usize / 5 {
            return Err("audio queue full".into());
        }
        s.pcm.extend(pcm);
        Ok(())
    }
    pub fn configure(
        &mut self,
        id: u32,
        gain: f32,
        at: Option<[f64; 3]>,
        radius: f32,
    ) -> Result<(), String> {
        if !gain.is_finite()
            || !(0.0..=2.0).contains(&gain)
            || !radius.is_finite()
            || !(1.0..=256.0).contains(&radius)
            || at.is_some_and(|p| p.iter().any(|x| !x.is_finite()))
        {
            return Err("invalid source configuration".into());
        }
        let mut m = self.mixer.lock().unwrap();
        let s = m.sources.get_mut(&id).ok_or("unknown source")?;
        s.gain = gain;
        s.at = at;
        s.radius = radius;
        Ok(())
    }
    pub fn remove(&mut self, id: u32) {
        self.mixer.lock().unwrap().sources.remove(&id);
    }
}
#[cfg(not(feature = "native-audio"))]
impl Sound {
    pub fn devices(&mut self) -> Result<Vec<(String, String)>, String> {
        Err("audio is disabled in this build".into())
    }
    pub fn start(&mut self, _device: Option<String>) -> Result<(u32, u16), String> {
        Err("audio is disabled in this build".into())
    }
    fn output(&mut self) -> Result<(), String> {
        Err("audio is disabled in this build".into())
    }
}
#[cfg(feature = "native-audio")]
enum Command {
    Devices(std::sync::mpsc::SyncSender<Result<Vec<(String, String)>, String>>),
    Start(
        Option<String>,
        std::sync::mpsc::SyncSender<Result<(u32, u16), String>>,
    ),
    Output(std::sync::mpsc::SyncSender<Result<(), String>>),
    Stop,
}
#[cfg(feature = "native-audio")]
impl Sound {
    fn worker(&mut self) -> Result<&std::sync::mpsc::SyncSender<Command>, String> {
        if std::env::var_os("HORIZON_NO_AUDIO").is_some() {
            return Err("audio disabled by HORIZON_NO_AUDIO".into());
        }
        if self.worker.is_none() {
            let (tx, rx) = std::sync::mpsc::sync_channel(8);
            let mixer = self.mixer.clone();
            std::thread::Builder::new()
                .name("Mod audio".into())
                .spawn(move || native::worker(rx, mixer))
                .map_err(|e| e.to_string())?;
            self.worker = Some(tx);
        }
        Ok(self.worker.as_ref().unwrap())
    }
    pub fn devices(&mut self) -> Result<Vec<(String, String)>, String> {
        let worker = self.worker()?.clone();
        request(&worker, &mut self.devices, Command::Devices)
    }
    pub fn start(&mut self, device: Option<String>) -> Result<(u32, u16), String> {
        if self.start.is_some() && self.starting_device != device {
            return Err("another microphone request is pending; stop it first".into());
        }
        self.starting_device = device.clone();
        let worker = self.worker()?.clone();
        request(&worker, &mut self.start, |tx| Command::Start(device, tx))
    }
    fn output(&mut self) -> Result<(), String> {
        if self.output_ready {
            return Ok(());
        }
        let worker = self.worker()?.clone();
        request(&worker, &mut self.output, Command::Output)?;
        self.output_ready = true;
        Ok(())
    }
}
#[cfg(feature = "native-audio")]
type Pending<T> = Option<std::sync::mpsc::Receiver<Result<T, String>>>;

/// Device setup must never wait on an OS audio service in a game hook.
#[cfg(feature = "native-audio")]
fn request<T>(
    worker: &std::sync::mpsc::SyncSender<Command>,
    pending: &mut Pending<T>,
    make: impl FnOnce(std::sync::mpsc::SyncSender<Result<T, String>>) -> Command,
) -> Result<T, String> {
    if pending.is_none() {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        worker.try_send(make(tx)).map_err(|e| e.to_string())?;
        *pending = Some(rx);
    }
    match pending.as_ref().unwrap().try_recv() {
        Ok(result) => {
            *pending = None;
            result
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            Err("audio operation pending; retry next frame".into())
        }
        Err(e) => {
            *pending = None;
            Err(e.to_string())
        }
    }
}
#[cfg(feature = "native-audio")]
mod native {
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    pub(super) fn worker(rx: std::sync::mpsc::Receiver<Command>, m: Arc<Mutex<Mixer>>) {
        let host = cpal::default_host();
        let mut input = None;
        let mut output = None;
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Command::Devices(tx) => {
                    let r = host.input_devices().map_err(|e| e.to_string()).map(|ds| {
                        ds.take(64)
                            .enumerate()
                            .map(|(i, d)| {
                                (
                                    i.to_string(),
                                    d.name().unwrap_or_else(|_| "Microphone".into()),
                                )
                            })
                            .collect()
                    });
                    let _ = tx.send(r);
                }
                Command::Start(id, tx) => {
                    input = None;
                    {
                        let mut m = m.lock().unwrap();
                        m.capture.clear();
                        m.capture_limit = 0;
                    }
                    let r = (|| {
                        let device = match id {
                            Some(id) => host
                                .input_devices()
                                .map_err(|e| e.to_string())?
                                .nth(id.parse::<usize>().map_err(|_| "unknown device")?)
                                .ok_or("unknown device")?,
                            None => host
                                .default_input_device()
                                .ok_or("no microphone available")?,
                        };
                        let config = device.default_input_config().map_err(|e| e.to_string())?;
                        let format = (config.sample_rate().0, config.channels());
                        if format.0 > 192000 || format.1 > 8 {
                            return Err("unsupported input format".into());
                        }
                        let stream = input_stream(&device, &config, m.clone())?;
                        m.lock().unwrap().capture_limit = format.0 as usize * format.1 as usize / 5;
                        stream.play().map_err(|e| e.to_string())?;
                        input = Some(stream);
                        Ok(format)
                    })();
                    // A canceled request must not leave a microphone open.
                    if tx.send(r).is_err() {
                        input = None;
                        let mut mixer = m.lock().unwrap();
                        mixer.capture_limit = 0;
                        mixer.capture.clear();
                    }
                }
                Command::Output(tx) => {
                    let r = if output.is_some() {
                        Ok(())
                    } else {
                        output_stream(&host, m.clone()).map(|s| output = Some(s))
                    };
                    let _ = tx.send(r);
                }
                Command::Stop => {
                    input = None;
                    let mut m = m.lock().unwrap();
                    m.capture_limit = 0;
                    m.capture.clear();
                }
            }
        }
        drop(input);
        drop(output);
    }
    fn input_stream(
        d: &cpal::Device,
        c: &cpal::SupportedStreamConfig,
        m: Arc<Mutex<Mixer>>,
    ) -> Result<cpal::Stream, String> {
        fn build<T: cpal::SizedSample>(
            d: &cpal::Device,
            c: &cpal::StreamConfig,
            m: Arc<Mutex<Mixer>>,
        ) -> Result<cpal::Stream, String>
        where
            f32: cpal::FromSample<T>,
        {
            d.build_input_stream(
                c,
                move |data: &[T], _| {
                    if let Ok(mut m) = m.try_lock() {
                        if m.capture_limit == 0 {
                            return;
                        }
                        for s in data {
                            if m.capture.len() >= m.capture_limit {
                                m.capture.pop_front();
                            }
                            m.capture.push_back(f32::from_sample(*s));
                        }
                    }
                },
                |e| log::warn!("mod microphone: {e}"),
                None,
            )
            .map_err(|e| e.to_string())
        }
        use cpal::Sample;
        match c.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(d, &c.config(), m),
            cpal::SampleFormat::I16 => build::<i16>(d, &c.config(), m),
            cpal::SampleFormat::U16 => build::<u16>(d, &c.config(), m),
            _ => Err("unsupported microphone sample format".into()),
        }
    }
    fn output_stream(h: &cpal::Host, m: Arc<Mutex<Mixer>>) -> Result<cpal::Stream, String> {
        fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
            d: &cpal::Device,
            c: &cpal::StreamConfig,
            m: Arc<Mutex<Mixer>>,
        ) -> Result<cpal::Stream, String> {
            let channels = c.channels as usize;
            let rate = c.sample_rate.0;
            d.build_output_stream(
                c,
                move |data: &mut [T], _| {
                    if let Ok(mut m) = m.try_lock() {
                        for frame in data.chunks_mut(channels) {
                            let s = m.frame(rate);
                            for (i, v) in frame.iter_mut().enumerate() {
                                *v = T::from_sample(if channels == 1 {
                                    (s[0] + s[1]) * 0.5
                                } else if i < 2 {
                                    s[i]
                                } else {
                                    0.0
                                });
                            }
                        }
                    } else {
                        data.fill(T::from_sample(0.0));
                    }
                },
                |e| log::warn!("mod audio: {e}"),
                None,
            )
            .map_err(|e| e.to_string())
        }
        let d = h
            .default_output_device()
            .ok_or("no audio output available")?;
        let c = d.default_output_config().map_err(|e| e.to_string())?;
        let s = match c.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&d, &c.config(), m),
            cpal::SampleFormat::I16 => build::<i16>(&d, &c.config(), m),
            cpal::SampleFormat::U16 => build::<u16>(&d, &c.config(), m),
            _ => Err("unsupported output sample format".into()),
        }?;
        s.play().map_err(|e| e.to_string())?;
        Ok(s)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mixer() -> Mixer {
        let mut m = Mixer::default();
        m.sources.insert(
            1,
            Source {
                rate: 48000,
                channels: 1,
                pcm: VecDeque::from(vec![1.0; 480]),
                fraction: 0.0,
                gain: 1.0,
                at: Some([0.0, 0.0, 2.0]),
                radius: 32.0,
            },
        );
        m
    }
    #[cfg(feature = "native-audio")]
    #[test]
    fn device_requests_are_nonblocking_and_coalesced() {
        let (tx, rx) = std::sync::mpsc::sync_channel(8);
        let mut pending = None;
        assert!(
            request(&tx, &mut pending, Command::Output)
                .unwrap_err()
                .contains("pending")
        );
        let Command::Output(reply) = rx.try_recv().unwrap() else {
            panic!("expected output request")
        };
        assert!(request(&tx, &mut pending, Command::Output).is_err());
        assert!(rx.try_recv().is_err());
        reply.send(Ok(())).unwrap();
        assert!(request(&tx, &mut pending, Command::Output).is_ok());
        assert!(pending.is_none());
    }
    #[test]
    fn positional_attenuation_and_starvation() {
        let mut m = mixer();
        let near = m.frame(48000);
        m.sources.get_mut(&1).unwrap().at = Some([0.0, 0.0, 64.0]);
        assert_eq!(m.frame(48000), [0.0; 2]);
        assert!(near[0] > 0.0);
        m.sources.get_mut(&1).unwrap().pcm.clear();
        assert_eq!(m.frame(48000), [0.0; 2]);
    }
    #[test]
    fn validates_and_bounds_pcm() {
        let mut s = Sound {
            mixer: Arc::new(Mutex::new(mixer())),
            ..Default::default()
        };
        assert!(s.push(1, vec![f32::NAN]).is_err());
        assert!(s.push(1, vec![0.0; 8193]).is_err());
        assert!(
            s.configure(1, 1.0, Some([f64::NAN, 0.0, 0.0]), 32.0)
                .is_err()
        );
        s.remove(1);
        assert!(s.push(1, vec![0.0]).is_err());
    }
}
