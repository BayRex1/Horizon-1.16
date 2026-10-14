#!/usr/bin/env python3
"""Check the production loading transitions without building either GUI.

Usage: python3 tools/check_loading_screens.py
Requires rustc; compiles the actual transition methods with minimal containers.
"""
import pathlib
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


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
use std::{cell::{Cell, RefCell}, path::PathBuf, sync::mpsc, time::Instant};
mod launcher {
    enum Load { Loading, Ready, Failed }
    struct Launcher { opening: Option<()>, releases: Load }
    impl Launcher { LAUNCHER }
    #[test]
    fn ready_and_failure_show_controls_in_the_same_frame() {
        for releases in [Load::Ready, Load::Failed] {
            let mut launcher = Launcher { opening: Some(()), releases };
            assert_eq!(launcher.opening_phase(), 1.0);
            assert!(launcher.opening.is_none());
            launcher.releases = Load::Loading;
            assert_eq!(launcher.opening_phase(), 1.0, "refresh must not reopen loading screen");
        }
    }
    #[test]
    fn pending_then_ready_has_no_reveal_delay() {
        let mut launcher = Launcher { opening: Some(()), releases: Load::Loading };
        for _ in 0..1000 { assert_eq!(launcher.opening_phase(), 0.0); }
        launcher.releases = Load::Ready;
        assert_eq!(launcher.opening_phase(), 1.0);
    }
    #[test]
    fn skip_and_disabled_preference_have_no_delay() {
        let mut launcher = Launcher { opening: Some(()), releases: Load::Loading };
        launcher.skip_opening();
        assert_eq!(launcher.opening_phase(), 1.0);
        let mut disabled = Launcher { opening: None, releases: Load::Loading };
        assert_eq!(disabled.opening_phase(), 1.0);
    }
}
mod front {
    #[derive(Clone, Copy, Debug)]
    pub struct Opening;
    OPENING
    impl super::Title { TITLE_TRANSITIONS }
    pub fn phase(title: &super::Title, ready: bool) -> f32 {
        title.opening_phase(ready)
    }
}
pub enum Mode { Main, Select }
struct WorldEntry;
struct BackupToast;
TITLE_STRUCT
impl Title { TITLE_NEW TITLE_ENTERED }
fn loading_title() -> Title {
    let title = Title::new(PathBuf::new());
    title.opening.set(Some(front::Opening::new()));
    title
}
#[test]
fn game_ready_on_first_frame_and_after_loading_has_no_delay() {
    let title = loading_title();
    assert_eq!(front::phase(&title, true), 1.0);
    assert!(title.opening.get().is_none());
    assert_eq!(front::phase(&title, false), 1.0);
    let title = loading_title();
    for _ in 0..1000 { assert_eq!(front::phase(&title, false), 0.0); }
    assert_eq!(front::phase(&title, true), 1.0);
}
#[test]
fn game_skip_and_bypassed_opening_have_no_delay() {
    let title = loading_title();
    assert!(title.skip_opening());
    assert_eq!(front::phase(&title, false), 1.0);
    assert!(!title.skip_opening(), "normal input must not be consumed after skip");
    let title = Title::new(PathBuf::new());
    assert_eq!(front::phase(&title, false), 1.0);
}
#[test]
fn first_page_does_not_fade_but_later_navigation_does() {
    let mut title = loading_title();
    assert!(title.entered().is_none());
    assert_eq!(front::phase(&title, true), 1.0);
    assert!(title.entered().is_none());
    title.mode = Mode::Select;
    let entered = title.entered();
    assert!(entered.is_some());
    assert_eq!(title.entered(), entered);
}
mod backdrop {
    use super::Instant;
    struct Backdrop { ready: Option<Instant> }
    struct Game { backdrop: Backdrop }
    impl Game { BACKDROP }
    #[test]
    fn backdrop_has_no_fade_after_readiness() {
        let mut game = Game { backdrop: Backdrop { ready: None } };
        assert_eq!(game.backdrop_fade(), 0.0);
        game.backdrop.ready = Some(Instant::now());
        assert_eq!(game.backdrop_fade(), 1.0);
    }
}
'''


def main():
    launcher = (ROOT / "launcher/src/main.rs").read_text()
    front = (ROOT / "crates/horizon-client/src/front.rs").read_text()
    title = (ROOT / "crates/horizon-client/src/title.rs").read_text()
    backdrop = (ROOT / "crates/horizon-client/src/game/backdrop.rs").read_text()
    code = HARNESS.replace("LAUNCHER", "\n".join(
        block(launcher, marker) for marker in ["fn opening_phase(", "fn skip_opening("]
    )).replace("OPENING", block(front, "impl Opening {")).replace(
        "TITLE_TRANSITIONS", "\n".join(block(front, marker) for marker in [
            "fn opening_phase(", "pub fn skip_opening("
        ])
    ).replace("TITLE_STRUCT", block(title, "pub struct Title {")).replace(
        "TITLE_NEW", block(title, "pub fn new(")
    ).replace("TITLE_ENTERED", block(title, "pub fn entered(")).replace(
        "BACKDROP", block(backdrop, "pub(super) fn backdrop_fade(")
    )
    # Keep production fields, including unrelated title caches, without warning on
    # their non-use in this focused test. Production builds check the full types.
    code = code.replace("crate::front::Opening", "front::Opening")
    with tempfile.TemporaryDirectory(prefix="horizon-loading-") as directory:
        path = pathlib.Path(directory)
        (path / "loading.rs").write_text(code)
        subprocess.run([
            "rustc", "--edition=2024", "--test", "-Dwarnings", "-Adead_code",
            str(path / "loading.rs"), "-o", str(path / "loading"),
        ], cwd=ROOT, check=True)
        subprocess.run([str(path / "loading")], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
