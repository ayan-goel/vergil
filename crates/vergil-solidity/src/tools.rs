use std::process::Command;

/// Information about an external tool Vergil depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInfo {
    /// Stable identifier used in code (e.g. "forge").
    pub name: &'static str,
    /// Human-friendly name for `vergil doctor` output (e.g. "Foundry (forge)").
    pub display_name: &'static str,
    /// Parsed version string, or `None` if the binary was not found or returned unparseable output.
    pub version: Option<String>,
    /// One-line install hint shown when the tool is missing.
    pub install_hint: &'static str,
}

impl ToolInfo {
    pub fn found(&self) -> bool {
        self.version.is_some()
    }
}

/// The set of external tools Vergil requires for Phase 1 verification.
///
/// Gambit (mutation testing) is intentionally excluded: it's a Phase 3 dependency.
pub fn detect() -> Vec<ToolInfo> {
    vec![
        detect_one(&FORGE),
        detect_one(&HALMOS),
        detect_one(&SLITHER),
        detect_one(&Z3),
        detect_one(&CVC5),
        detect_one(&SOLC),
    ]
}

struct ToolSpec {
    name: &'static str,
    display_name: &'static str,
    cmd: &'static str,
    args: &'static [&'static str],
    parser: fn(&str) -> Option<String>,
    install_hint: &'static str,
}

fn detect_one(spec: &ToolSpec) -> ToolInfo {
    let version = run_and_parse(spec);
    ToolInfo {
        name: spec.name,
        display_name: spec.display_name,
        version,
        install_hint: spec.install_hint,
    }
}

fn run_and_parse(spec: &ToolSpec) -> Option<String> {
    let output = Command::new(spec.cmd).args(spec.args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(v) = (spec.parser)(&stdout) {
        return Some(v);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    (spec.parser)(&stderr)
}

const FORGE: ToolSpec = ToolSpec {
    name: "forge",
    display_name: "Foundry (forge)",
    cmd: "forge",
    args: &["--version"],
    parser: parse_forge,
    install_hint: "curl -L https://foundry.paradigm.xyz | bash && foundryup",
};

const HALMOS: ToolSpec = ToolSpec {
    name: "halmos",
    display_name: "Halmos",
    cmd: "halmos",
    args: &["--version"],
    parser: parse_halmos,
    install_hint: "uv tool install halmos==0.3.3",
};

const SLITHER: ToolSpec = ToolSpec {
    name: "slither",
    display_name: "Slither",
    cmd: "slither",
    args: &["--version"],
    parser: parse_slither,
    install_hint: "uv tool install slither-analyzer==0.11.0",
};

const Z3: ToolSpec = ToolSpec {
    name: "z3",
    display_name: "Z3",
    cmd: "z3",
    args: &["--version"],
    parser: parse_z3,
    install_hint: "brew install z3   # or: apt-get install z3",
};

const CVC5: ToolSpec = ToolSpec {
    name: "cvc5",
    display_name: "cvc5",
    cmd: "cvc5",
    args: &["--version"],
    parser: parse_cvc5,
    install_hint: "brew tap cvc5/cvc5 && brew install --cask cvc5/cvc5/cvc5",
};

const SOLC: ToolSpec = ToolSpec {
    name: "solc",
    display_name: "solc",
    cmd: "solc",
    args: &["--version"],
    parser: parse_solc,
    install_hint: "brew install solidity   # or: apt-get install solc",
};

fn parse_forge(out: &str) -> Option<String> {
    for line in out.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("forge Version:") {
            return Some(rest.trim().to_string());
        }
        if let Some(rest) = line.strip_prefix("forge ") {
            let v = rest.trim_start_matches('v').trim();
            if v.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return Some(v.split_whitespace().next()?.to_string());
            }
        }
    }
    None
}

fn parse_halmos(out: &str) -> Option<String> {
    let line = out.lines().next()?.trim();
    let v = line.strip_prefix("halmos")?.trim();
    if v.is_empty() {
        return None;
    }
    Some(v.split_whitespace().next()?.to_string())
}

fn parse_slither(out: &str) -> Option<String> {
    let v = out.lines().next()?.trim();
    if v.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Some(v.to_string())
    } else {
        None
    }
}

fn parse_z3(out: &str) -> Option<String> {
    let line = out.lines().next()?.trim();
    let v = line.strip_prefix("Z3 version")?.trim();
    Some(v.split_whitespace().next()?.to_string())
}

fn parse_cvc5(out: &str) -> Option<String> {
    let line = out.lines().next()?.trim();
    // cvc5 has at least two version-line shapes:
    //   "cvc5 1.3.4 [git ...]"                       (older builds, brew on macOS)
    //   "This is cvc5 version 1.3.0 [git tag ...]"   (1.3.0 Linux static release)
    let after = line
        .strip_prefix("This is cvc5 version")
        .or_else(|| line.strip_prefix("cvc5"))?
        .trim();
    Some(after.split_whitespace().next()?.to_string())
}

fn parse_solc(out: &str) -> Option<String> {
    for line in out.lines() {
        if let Some(rest) = line.trim().strip_prefix("Version:") {
            let v = rest.trim();
            return Some(v.split('+').next().unwrap_or(v).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn forge_version_parses() {
        let out = indoc! {"
            forge Version: 1.7.1
            Commit SHA: 4072e48705af9d93e3c0f6e29e93b5e9a40caed8
            Build Timestamp: 2024-12-12T10:00:00Z
        "};
        assert_eq!(parse_forge(out), Some("1.7.1".to_string()));
    }

    #[test]
    fn forge_older_format_parses() {
        let out = "forge 0.2.0 (a1b2c3 2024-01-01)\n";
        assert_eq!(parse_forge(out), Some("0.2.0".to_string()));
    }

    #[test]
    fn forge_garbage_rejects() {
        assert_eq!(parse_forge("forge: command not found\n"), None);
        assert_eq!(parse_forge(""), None);
    }

    #[test]
    fn halmos_version_parses() {
        assert_eq!(parse_halmos("halmos 0.3.3\n"), Some("0.3.3".to_string()));
    }

    #[test]
    fn halmos_with_extra_output_parses() {
        let out = "halmos 0.3.3 (bundled solc 0.8.20)\n";
        assert_eq!(parse_halmos(out), Some("0.3.3".to_string()));
    }

    #[test]
    fn halmos_garbage_rejects() {
        assert_eq!(parse_halmos("not halmos\n"), None);
        assert_eq!(parse_halmos(""), None);
    }

    #[test]
    fn slither_version_parses() {
        assert_eq!(parse_slither("0.11.0\n"), Some("0.11.0".to_string()));
    }

    #[test]
    fn slither_garbage_rejects() {
        assert_eq!(parse_slither("error: foo\n"), None);
        assert_eq!(parse_slither(""), None);
    }

    #[test]
    fn z3_version_parses() {
        assert_eq!(
            parse_z3("Z3 version 4.15.4 - 64 bit\n"),
            Some("4.15.4".to_string())
        );
    }

    #[test]
    fn z3_garbage_rejects() {
        assert_eq!(parse_z3("z3: error\n"), None);
    }

    #[test]
    fn cvc5_version_parses_old_format() {
        let out = indoc! {"
            cvc5 1.3.4 [git f3b21c4 on branch HEAD]
            compiled with GCC version Apple LLVM 17.0.0 on May  7 2026
        "};
        assert_eq!(parse_cvc5(out), Some("1.3.4".to_string()));
    }

    #[test]
    fn cvc5_version_parses_new_format() {
        // Linux static release of cvc5 1.3.0 emits this longer header.
        let out = indoc! {"
            This is cvc5 version 1.3.0 [git tag 1.3.0 branch HEAD]
            compiled with GCC version 11.4.0
        "};
        assert_eq!(parse_cvc5(out), Some("1.3.0".to_string()));
    }

    #[test]
    fn cvc5_garbage_rejects() {
        assert_eq!(parse_cvc5("cvcOther 0.1\n"), None);
    }

    #[test]
    fn solc_version_parses() {
        let out = indoc! {"
            solc, the solidity compiler commandline interface
            Version: 0.8.35+commit.47b9dedd.Darwin.appleclang
        "};
        assert_eq!(parse_solc(out), Some("0.8.35".to_string()));
    }

    #[test]
    fn solc_version_without_plus_parses() {
        let out = "Version: 0.8.20\n";
        assert_eq!(parse_solc(out), Some("0.8.20".to_string()));
    }

    #[test]
    fn solc_garbage_rejects() {
        assert_eq!(parse_solc("solc: command not found\n"), None);
    }
}
