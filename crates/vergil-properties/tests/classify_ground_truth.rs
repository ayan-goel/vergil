//! V1.5 Phase 3 Slice 6 — SPEC §11.3 exit gate.
//!
//! Loads `vergilbench/primitives-ground-truth.yaml`, runs the
//! classifier against every contract in `vergilbench/contracts/`, and
//! asserts ≥90% accuracy (≥90/100 correct calls).
//!
//! ## Correctness definition
//!
//! For each bench contract:
//! - Ground-truth `[]` AND classifier returns empty → correct
//! - Ground-truth `[X, ...]` AND classifier's top match is one of the
//!   labeled primitives → correct
//! - Otherwise → wrong (false positive on a utility lib, false
//!   negative on a real primitive, or mis-classified primitive)
//!
//! ## Gating
//!
//! `solc` must be on PATH for storage-layout signals. The test skips
//! cleanly otherwise (CI without Solidity toolchain). **No LLM key
//! required** — Phase 3 is pure static analysis.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use vergil_properties::classify::{classify, ClassifyConfig, Primitive};
use vergil_solidity::storage::{run_simple, StorageLayout, StorageResult};

#[derive(Debug, Deserialize)]
struct GroundTruth {
    contracts: Vec<ContractLabel>,
}

#[derive(Debug, Deserialize)]
struct ContractLabel {
    name: String,
    primitives: Vec<String>,
}

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/vergil-properties
    p.pop(); // crates
    p
}

fn solc_available() -> bool {
    std::process::Command::new("solc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn load_ground_truth() -> GroundTruth {
    let path = workspace_root().join("vergilbench/primitives-ground-truth.yaml");
    let body =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_yaml::from_str(&body).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_sources(project: &Path) -> String {
    let src_dir = project.join("src");
    let Ok(rd) = std::fs::read_dir(&src_dir) else {
        return String::new();
    };
    let mut joined = String::new();
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("sol"))
        .collect();
    files.sort();
    for p in files {
        if let Ok(s) = std::fs::read_to_string(&p) {
            joined.push_str(&s);
            joined.push('\n');
        }
    }
    joined
}

async fn load_layouts(project: &Path) -> Vec<StorageLayout> {
    let src_dir = project.join("src");
    let Ok(rd) = std::fs::read_dir(&src_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("sol"))
        .collect();
    paths.sort();
    for p in paths {
        match run_simple(&p, Duration::from_secs(30)).await {
            StorageResult::Ok(mut l) => out.append(&mut l),
            StorageResult::Error(_) => {
                // Many bench contracts are libraries with no storage —
                // solc returns "no contracts" or similar; treat as
                // empty layout (the classifier doesn't need layouts
                // for libraries).
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn ground_truth_accuracy() {
    if !solc_available() {
        eprintln!("classify_ground_truth: solc not on PATH — skipping");
        return;
    }
    let gt = load_ground_truth();
    let contracts_root = workspace_root().join("vergilbench/contracts");
    let cfg = ClassifyConfig::default();

    let mut correct = 0usize;
    let mut total = 0usize;
    let mut misses: Vec<String> = Vec::new();

    for label in &gt.contracts {
        let project = contracts_root.join(&label.name);
        if !project.is_dir() {
            misses.push(format!("{}: project dir missing", label.name));
            total += 1;
            continue;
        }
        let source = load_sources(&project);
        if source.is_empty() {
            misses.push(format!("{}: no .sol sources", label.name));
            total += 1;
            continue;
        }
        let layouts = load_layouts(&project).await;
        let report = classify(&source, &layouts, &cfg);

        let expected: Vec<Primitive> = label
            .primitives
            .iter()
            .filter_map(|s| Primitive::from_id(s))
            .collect();
        let is_correct = match (expected.is_empty(), report.top()) {
            (true, None) => true,
            (true, Some(_)) => false, // false positive on a utility lib
            (false, None) => false,   // false negative on a real primitive
            (false, Some(top)) => expected.contains(&top.primitive),
        };
        if is_correct {
            correct += 1;
        } else {
            let predicted = report
                .top()
                .map(|m| format!("{} ({:.2})", m.primitive.id(), m.confidence))
                .unwrap_or_else(|| "<none>".into());
            misses.push(format!(
                "{}: expected {:?}, predicted {}",
                label.name, label.primitives, predicted
            ));
        }
        total += 1;
    }

    let pct = if total > 0 {
        (correct as f32 / total as f32) * 100.0
    } else {
        0.0
    };
    eprintln!("classify_ground_truth: {correct}/{total} correct ({pct:.1}%)");
    if !misses.is_empty() {
        eprintln!("--- misses ---");
        for m in &misses {
            eprintln!("  - {m}");
        }
    }

    assert!(
        total == 100,
        "ground truth must label all 100 bench contracts; saw {total}"
    );
    assert!(
        correct >= 90,
        "SPEC §11.3 exit gate: expected ≥90/100, got {correct}/{total} ({pct:.1}%)"
    );
}
