//! Phase 4 Slice 8 — verifies the `--no-tests` and `--no-natspec` flags
//! exist on the `vergil verify` surface per SPEC §3.7. The actual
//! routing of these flags into the extraction pipeline is Phase 6 work
//! (the standardized-workflow stratified-verdict UI), per the project
//! handoff §1 / §7.
//!
//! These tests shell out to the built CLI rather than parsing the clap
//! tree in-process so we exercise the same surface a user would.

use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

fn cargo_run_help() -> std::process::Output {
    Command::new(env!("CARGO"))
        .args([
            "run",
            "-p",
            "vergil-cli",
            "--bin",
            "vergil",
            "--quiet",
            "--",
            "verify",
            "--help",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("cargo run vergil verify --help")
}

#[test]
fn verify_help_lists_no_tests_and_no_natspec_flags() {
    let out = cargo_run_help();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("--no-tests"),
        "--no-tests flag missing from `vergil verify --help`:\n{stdout}"
    );
    assert!(
        stdout.contains("--no-natspec"),
        "--no-natspec flag missing from `vergil verify --help`:\n{stdout}"
    );
    // Help text should hint that Phase 6 owns the wiring so users
    // aren't surprised the flag is a no-op today.
    assert!(
        stdout.contains("Phase 6") || stdout.contains("forward-compat"),
        "help text should call out Phase 6 forward-compat semantics:\n{stdout}"
    );
}

#[test]
fn verify_accepts_no_tests_flag_without_error() {
    // The flag must PARSE cleanly even if it's not yet wired through.
    // We don't supply a real path — the command will fail downstream
    // (no foundry.toml), but clap parsing must succeed BEFORE that.
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO"))
        .args([
            "run",
            "-p",
            "vergil-cli",
            "--bin",
            "vergil",
            "--quiet",
            "--",
            "verify",
            tmp.path().to_str().unwrap(),
            "--no-tests",
            "--no-natspec",
            "--mode",
            "zero-config",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("cargo run vergil verify --no-tests");
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Clap rejection prints "error:" prefixed messages on stderr. The
    // command itself may fail downstream (no foundry.toml), but the
    // FLAG parsing must succeed.
    assert!(
        !stderr.contains("error: unexpected argument"),
        "clap rejected --no-tests / --no-natspec:\n{stderr}"
    );
    assert!(
        !stderr.contains("error: invalid value"),
        "clap rejected a flag value:\n{stderr}"
    );
}
