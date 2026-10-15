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
use std::time::Instant;
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
    backdrop = (ROOT / "crates/horizon-client/src/game/backdrop.rs").read_text()
    code = HARNESS.replace("LAUNCHER", "\n".join(
        block(launcher, marker) for marker in ["fn opening_phase(", "fn skip_opening("]
    )).replace(
        "BACKDROP", block(backdrop, "pub(super) fn backdrop_fade(")
    )
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
