//! Offline CLI/MCP play reaches resource preparation without an account or a session file.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const REFUSED: &str = "Sign in with a Microsoft account that owns Minecraft: Java Edition to play.";

/// Fresh homes with one installed (not prepared) version and an offline name, as a player who
/// played offline before the gate has.
fn home(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("horizon-owner-gate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let version = root.join("launcher/versions/v2026.10.27.01");
    std::fs::create_dir_all(&version).unwrap();
    std::fs::write(version.join(".installed"), "").unwrap();
    std::fs::write(
        root.join("launcher/accounts.json"),
        r#"{"accounts":[],"selected":null,"offline":["Builder_1"],"offline_selected":"Builder_1"}"#,
    )
    .unwrap();
    root
}

fn launcher(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_horizon-launcher"));
    cmd.args(args)
        .env("HORIZON_HOME", root.join("game"))
        .env("HORIZON_LAUNCHER_HOME", root.join("launcher"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("HORIZON_LAUNCHER_TRASH", root.join("trash"))
        .env("HORIZON_LAUNCHER_CLIPBOARD", root.join("clipboard"))
        .env("HORIZON_LAUNCHER_DESKTOP", root.join("desktop"))
        .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
        .env("HORIZON_NO_AUDIO", "1")
        .env("HORIZON_TELEMETRY_URL", "off")
        // Memory only: the test never reads the player's keychain.
        .env("HORIZON_NO_KEYCHAIN", "1")
        .env("HORIZON_MSA_CLIENT_ID", "test-client");
    cmd
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

#[test]
fn the_cli_allows_offline_play_without_an_owner() {
    let root = home("cli");
    let out = launcher(&root, &["play"]).output().unwrap();
    let said = text(&out);
    assert!(!out.status.success(), "{said}");
    assert!(!said.contains(REFUSED), "{said}");
    assert!(said.contains("not prepared yet"), "{said}");
    // Preparation still requires the player's own resources; no session is fabricated.
    assert!(!root.join("game/sessions").exists());
    let accounts = std::fs::read_to_string(root.join("launcher/accounts.json")).unwrap();
    assert!(accounts.contains("Builder_1"), "{accounts}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_mcp_server_allows_offline_play_without_an_owner() {
    let root = home("mcp");
    let mut child = launcher(&root, &["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"play","arguments":{}}}"#,
    ];
    let mut stdin = child.stdin.take().unwrap();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let reply = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["id"] == 2)
        .expect("a reply to play");
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let said = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert!(!said.contains(REFUSED), "{said}");
    assert!(said.contains("not prepared yet"), "{said}");
    assert!(!root.join("game/sessions").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_fresh_install_and_a_build_without_microsoft_configuration_can_play_offline() {
    let root = home("fresh");
    std::fs::remove_file(root.join("launcher/accounts.json")).unwrap();
    let out = launcher(&root, &["play"])
        .env("HORIZON_MSA_CLIENT_ID", "")
        .output()
        .unwrap();
    let said = text(&out);
    assert!(said.contains("not prepared yet"), "{said}");
    assert!(!said.contains(REFUSED), "{said}");
    assert!(!root.join("game/sessions").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn offline_profile_selection_does_not_require_a_keychain_owner() {
    let root = home("profile");
    let out = launcher(&root, &["account", "offline", "Builder_2"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let accounts: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("launcher/accounts.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(accounts["offline_selected"], "Builder_2");
    let _ = std::fs::remove_dir_all(&root);
}
