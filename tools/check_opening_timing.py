#!/usr/bin/env python3
"""Run both production opening transitions with a 60 Hz virtual clock.

Usage: python3 tools/check_opening_timing.py <baseline-git-ref>
Requires rustc and git; no GUI, resources, or Rust dependencies are needed.
"""
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


def source(path, ref):
    if ref:
        return subprocess.check_output(
            ["git", "show", f"{ref}:{path}"], cwd=ROOT, text=True
        )
    return (ROOT / path).read_text()


def block(text, marker):
    start = text.index(marker)
    brace = text.index("{", start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


HARNESS = r'''
use std::{cell::Cell, hint::black_box, sync::atomic::{AtomicU64, Ordering}, time::Duration};
static NOW: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy, Debug)]
struct Instant(u64);
impl Instant {
    fn now() -> Self { Self(NOW.load(Ordering::Relaxed)) }
    fn elapsed(self) -> Duration {
        Duration::from_nanos(NOW.load(Ordering::Relaxed) - self.0)
    }
}
#[allow(dead_code)]
mod horizon_theme { use super::*; THEME }
mod launcher {
    use super::*;
    const OPENING_HOLD: Duration = horizon_theme::OPENING_HOLD;
    const OPENING_MAX: Duration = horizon_theme::OPENING_MAX;
    const OPENING_REVEAL: Duration = horizon_theme::OPENING_REVEAL;
    fn ease_out_cubic(t: f32) -> f32 { 1. - (1. - t.clamp(0., 1.)).powi(3) }
    struct Opening { started: Option<Instant>, reveal: Option<Instant> }
    enum Load { Loading, Ready }
    struct Window;
    impl Window { fn request_animation_frame(&mut self) {} }
    pub struct Launcher { opening: Option<Opening>, releases: Load }
    impl Launcher {
        pub fn new(skip: bool) -> Self {
            Self { opening: (!skip).then_some(Opening { started: None, reveal: None }), releases: Load::Loading }
        }
        pub fn tick(&mut self, ready: bool, skip: bool) -> f32 {
            self.releases = if ready { Load::Ready } else { Load::Loading };
            if skip { self.skip_opening(); }
            self.opening_phase(&mut Window).0
        }
        LAUNCHER
    }
}
mod game {
    use super::*;
    #[derive(Clone, Copy, Debug)]
    pub struct Opening { started: Instant, reveal: Option<Instant> }
    GAME_OPENING
    pub struct Title { opening: Cell<Option<Opening>> }
    impl Title {
        pub fn new() -> Self { Self { opening: Cell::new(Some(Opening::new())) } }
        pub fn tick(&self, ready: bool, skip: bool) -> f32 {
            if skip { self.skip_opening(); }
            self.opening_phase(ready)
        }
        GAME_TITLE
    }
}
fn main() {
    let frame = 16_666_667u64;
    for (scenario, ready_ms, skip_ms, disabled) in [
        ("ready", 0, u64::MAX, false),
        ("ready_500ms", 500, u64::MAX, false),
        ("never_ready", u64::MAX, u64::MAX, false),
        ("input_skip_100ms", u64::MAX, 100, false),
        ("launcher_skip_setting", u64::MAX, u64::MAX, true),
    ] {
        for path in ["launcher", "game"] {
            if disabled && path == "game" { continue; }
            let mut results = Vec::new();
            for repetition in 0..32 {
                NOW.store(0, Ordering::Relaxed);
                let mut launcher = launcher::Launcher::new(disabled);
                let game = game::Title::new();
                let mut previous = 0f32;
                for tick in 0..300 {
                    let ns = tick * frame;
                    NOW.store(ns, Ordering::Relaxed);
                    let phase = black_box(if path == "launcher" {
                        launcher.tick(black_box(ns / 1_000_000 >= ready_ms), ns / 1_000_000 >= skip_ms)
                    } else {
                        game.tick(black_box(ns / 1_000_000 >= ready_ms), ns / 1_000_000 >= skip_ms)
                    });
                    assert!((previous..=1.0).contains(&phase));
                    previous = phase;
                    if phase >= 1.0 {
                        let ceil = |ns: u64| ns.div_ceil(frame) * frame;
                        let hold = horizon_theme::OPENING_HOLD.as_nanos() as u64;
                        let max = horizon_theme::OPENING_MAX.as_nanos() as u64;
                        let ready = ready_ms.saturating_mul(1_000_000);
                        let skip = skip_ms.saturating_mul(1_000_000);
                        let start = ceil(hold.max(ready).min(max).min(skip));
                        let reveal = horizon_theme::OPENING_REVEAL.as_nanos() as u64;
                        let expected = if disabled { 0 } else { ceil(start + reveal) };
                        assert_eq!(ns, expected, "{path} {scenario}: timing or readiness changed");
                        if repetition > 0 { results.push(ns); }
                        break;
                    }
                }
            }
            assert_eq!(results.len(), 31, "transition never finished");
            results.sort_unstable();
            println!("{path},{scenario},{:.3}", results[15] as f64 / 1_000_000.0);
        }
    }
}
'''


def run(ref, label):
    theme = source("crates/horizon-theme/src/lib.rs", ref)
    theme = "\n".join(line for line in theme.splitlines() if line.startswith(
        ("pub const QUICK:", "pub const MOVE:", "pub const OPENING_")
    ))
    launcher = source("launcher/src/main.rs", ref)
    game = source("crates/horizon-client/src/front.rs", ref)
    code = HARNESS.replace("THEME", theme).replace("LAUNCHER", "\n".join(
        block(launcher, marker) for marker in ["fn opening_phase(", "fn skip_opening("]
    )).replace("GAME_OPENING", block(game, "impl Opening {")).replace(
        "GAME_TITLE", "\n".join(block(game, marker) for marker in [
            "fn opening_phase(", "pub fn skip_opening("
        ])
    )
    with tempfile.TemporaryDirectory(prefix="horizon-opening-") as directory:
        path = pathlib.Path(directory)
        (path / "timing.rs").write_text(code)
        subprocess.run(["rustc", "--edition=2021", "-O", "-Dwarnings", str(path / "timing.rs"),
                        "-o", str(path / "timing")], cwd=ROOT, check=True)
        print(f"{label}: path,scenario,median_full_reveal_ms (31 virtual-clock runs)", flush=True)
        subprocess.run([str(path / "timing")], cwd=ROOT, check=True)


if __name__ == "__main__":
    run(sys.argv[1] if len(sys.argv) > 1 else "HEAD", "baseline")
    run(None, "working tree")
