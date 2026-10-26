//! Real WebAssembly boundary tests; no game callbacks during hover.
use horizon_mods::{EnchantmentSource, Mods, Output, Package, Status, manifest::componentize};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

fn component() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target = root.join("target/tooltip-test");
        assert!(
            Command::new(env!("CARGO"))
                .current_dir(root)
                .args([
                    "build",
                    "--release",
                    "--target",
                    "wasm32-unknown-unknown",
                    "-p",
                    "horizon-mods-test-tooltip-guest",
                    "--target-dir",
                ])
                .arg(&target)
                .status()
                .unwrap()
                .success()
        );
        componentize(
            &std::fs::read(
                target.join("wasm32-unknown-unknown/release/horizon_mods_test_tooltip_guest.wasm"),
            )
            .unwrap(),
        )
        .unwrap()
    })
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "horizon-tooltip-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn package(&self, id: &str, capabilities: &str) -> PathBuf {
        let manifest = format!(
            "id = {id:?}\nname = {id:?}\nversion = \"0.1.0\"\napi = \"0.1\"\ncapabilities = [{capabilities}]\nexperimental = [\"tooltip\"]\n"
        );
        let path = self.0.join(format!("{id}.hmod"));
        std::fs::write(&path, Package::write(&manifest, component()).unwrap()).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn load(paths: &[PathBuf]) -> Mods {
    Mods::load(paths, |_| true, |_| false)
}
fn count(mods: &Mods) -> usize {
    mods.tooltip_rules()
        .map(|r| {
            r.item("minecraft:stick").len()
                + r.enchantment(EnchantmentSource::Applied, "minecraft:mending")
                    .len()
                + r.enchantment(EnchantmentSource::Stored, "minecraft:mending")
                    .len()
        })
        .sum()
}
fn touch(path: &Path) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
        .unwrap();
}
const CAPS: &str = "\"tooltip\", \"storage\"";

#[test]
fn real_guest_registers_replaces_and_clears_rules() {
    let imports = horizon_mods::api_level::imports(component()).unwrap();
    assert_eq!(
        horizon_mods::api_level::needed(imports.iter().map(String::as_str)).0,
        4
    );
    let f = Fixture::new();
    let mut mods = load(&[f.package("tooltips", CAPS)]);
    assert_eq!(mods.entries()[0].status, Status::Running);
    assert_eq!(count(&mods), 3);
    let rules = mods.tooltip_rules().next().unwrap();
    assert_eq!(rules.item("minecraft:stick")[0].color, Some(0x123456));
    assert_eq!(
        rules.enchantment(EnchantmentSource::Stored, "minecraft:mending")[0].translation_key,
        "tooltip.fixture.stored"
    );
    assert!(mods.command("tooltip replace"));
    assert_eq!(count(&mods), 1);
    assert!(mods.command("tooltip clear"));
    assert_eq!(count(&mods), 0);
}

#[test]
fn invalid_replacement_preserves_previous_rules() {
    let f = Fixture::new();
    let mut mods = load(&[f.package("atomic", CAPS)]);
    for bad in ["duplicate", "invalid", "color", "quota"] {
        assert!(mods.command(&format!("tooltip {bad}")));
        assert!(
            mods.drain()
                .iter()
                .any(|o| matches!(o, Output::Feedback { error: true, .. })),
            "{bad}"
        );
        assert_eq!(mods.entries()[0].status, Status::Running);
        assert_eq!(count(&mods), 3, "{bad}");
    }
}

#[test]
fn undeclared_tooltip_capability_cannot_register() {
    let f = Fixture::new();
    let mods = load(&[f.package("denied", "\"storage\"")]);
    assert!(matches!(&mods.entries()[0].status, Status::Failed(e) if e.contains("tooltip")));
    assert_eq!(count(&mods), 0);
}

#[test]
fn disable_remove_and_reload_discard_instance_rules() {
    let f = Fixture::new();
    let path = f.package("lifecycle", CAPS);
    let off = Mods::load(std::slice::from_ref(&path), |_| false, |_| false);
    assert_eq!(off.entries()[0].status, Status::Off);
    assert_eq!(count(&off), 0);
    let mut mods = load(std::slice::from_ref(&path));
    mods.command("tooltip replace");
    touch(&path);
    assert!(mods.reload_changed(|_| false));
    assert_eq!(count(&mods), 3);
    std::fs::remove_file(path).unwrap();
    assert!(mods.reload_changed(|_| false));
    assert_eq!(count(&mods), 0);
}

#[test]
fn failed_init_and_trap_do_not_leak_rules_or_affect_other_mods() {
    let f = Fixture::new();
    let path = f.package("first", CAPS);
    let mut mods = load(std::slice::from_ref(&path));
    mods.command("tooltip fail_reload");
    touch(&path);
    assert!(mods.reload_changed(|_| false));
    assert!(
        matches!(&mods.entries()[0].status, Status::Failed(e) if e.contains("fixture init failed"))
    );
    assert_eq!(count(&mods), 0);
    // A fresh fixture avoids persisted fail-init storage.
    let f = Fixture::new();
    let mut mods = load(&[f.package("first", CAPS), f.package("second", CAPS)]);
    assert_eq!(count(&mods), 6);
    mods.command("tooltip clear");
    assert_eq!(count(&mods), 3);
    mods.command("tooltip trap");
    assert_eq!(count(&mods), 3);
}

/// Point at a retained core module compiled with the SDK before tooltip existed.
/// Explicit to avoid quietly rebuilding an old source against the current SDK.
#[test]
#[ignore = "requires HORIZON_PRE_TOOLTIP_WASM from a pinned pre-tooltip SDK build"]
fn previously_compiled_guest_loads_alongside_tooltip_guest() {
    let f = Fixture::new();
    let core = std::fs::read(
        std::env::var_os("HORIZON_PRE_TOOLTIP_WASM").expect("precompiled old guest path"),
    )
    .unwrap();
    let old = f.0.join("old.hmod");
    let manifest = "id = \"old\"\nname = \"Old SDK\"\nversion = \"0.1.0\"\napi = \"0.1\"\ncapabilities = [\"chat\", \"storage\"]\n";
    std::fs::write(
        &old,
        Package::write(manifest, &componentize(&core).unwrap()).unwrap(),
    )
    .unwrap();
    let mut mods = load(&[old, f.package("new", CAPS)]);
    assert!(mods.entries().iter().all(|e| e.status == Status::Running));
    assert!(mods.command("hello compatibility"));
    assert!(
        mods.drain()
            .iter()
            .any(|o| matches!(o, Output::Chat { text, .. } if text.contains("compatibility")))
    );
    assert_eq!(count(&mods), 3);
}
