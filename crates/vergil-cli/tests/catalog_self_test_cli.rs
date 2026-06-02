//! V1.5 Phase 6 Slice 0 — `vergil catalog self-test` exists as a
//! subcommand under `vergil catalog` and runs the catalog-self-test
//! loop that Phase 1 used to ship under `vergil verify --mode zero-config`.
//!
//! Per `tasks/plan.md` §4 Slice 0 the relocation frees the `verify`
//! `--mode zero-config` slot so Phase 6 Slice 8 can repurpose it as the
//! Stage 1 oracle path. Until Slice 8 lands the verify-mode change,
//! `vergil verify --mode zero-config` prints a redirect message
//! pointing users at `vergil catalog self-test`.

use std::path::Path;
use std::process::Command;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
}

fn vergil(args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args([
        "run",
        "-p",
        "vergil-cli",
        "--bin",
        "vergil",
        "--quiet",
        "--",
    ])
    .args(args)
    .current_dir(workspace_root());
    cmd.output().expect("cargo run vergil")
}

#[test]
fn catalog_help_lists_self_test_subcommand() {
    let out = vergil(&["catalog", "--help"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("self-test"),
        "`vergil catalog --help` should advertise the `self-test` subcommand:\n{stdout}"
    );
}

#[test]
fn catalog_self_test_runs_against_erc20_example() {
    let project = workspace_root().join("examples/erc20");
    assert!(
        project.join("foundry.toml").is_file(),
        "examples/erc20 must exist as a Foundry project for this test to run"
    );
    let out = vergil(&["catalog", "self-test", project.to_str().expect("utf8 path")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Output must include the activation summary, matching the Phase-1
    // zero_config command's structure verbatim — header + a per-template
    // ✓/✗ table + a Summary section.
    assert!(
        stdout.contains("catalog-self-test")
            || stdout.contains("Zero-config")
            || stdout.contains("self-test"),
        "self-test output missing the header banner:\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stdout.contains("Activated templates"),
        "self-test output missing activation summary:\nstdout={stdout}"
    );
    assert!(
        stdout.contains("Summary"),
        "self-test output missing Summary section:\nstdout={stdout}"
    );
    // The exit status must succeed on a clean example.
    assert!(
        out.status.success(),
        "catalog self-test failed against examples/erc20:\nstdout={stdout}\nstderr={stderr}"
    );
}

#[test]
fn verify_mode_zero_config_invokes_unified_runner() {
    // Slice 8 repurposed `verify --mode zero-config` from the Phase-1
    // catalog-self-test loop (now at `vergil catalog self-test`) to
    // the Stage 1 oracle path feeding the stratified verdict. We
    // exercise that via `--list-applicable` so the test doesn't
    // require an Anthropic API key — the short-circuit prints the
    // fingerprint + activation summary and exits 0 without LLM calls.
    let project = workspace_root().join("examples/erc20");
    let out = vergil(&[
        "verify",
        project.to_str().expect("utf8 path"),
        "--mode",
        "zero-config",
        "--list-applicable",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "`vergil verify --mode zero-config --list-applicable` should exit 0:\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stdout.contains("vergil verify --list-applicable")
            && stdout.contains("Activated attack-catalog templates"),
        "unified runner --list-applicable output missing:\n{stdout}"
    );
}
