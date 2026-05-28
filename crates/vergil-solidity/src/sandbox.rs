//! Subprocess sandbox primitives — Phase 4 Slice B1.
//!
//! Wraps long-running solver subprocesses (halmos, solc, forge, slither)
//! in a platform-native sandbox that denies network egress and confines
//! filesystem writes to the project dir + any explicitly-allowed cache
//! paths. Read access stays broad because the subprocesses need system
//! headers, libc, and the various Solidity toolchain caches.
//!
//! Two backends:
//!   * `MacosSandboxExec` — invokes `sandbox-exec -p <profile> <cmd>`
//!     with a profile string assembled per call from [`SandboxConfig`].
//!   * `LinuxBubblewrap` — invokes `bwrap` with explicit bind-mounts
//!     + `--unshare-net` when network is denied.
//!
//! [`SandboxKind::None`] is the fallback for unsupported platforms or
//! when the operator explicitly opts out via `--no-sandbox`. It runs the
//! command unwrapped and emits one `tracing::warn` so ops sees the gap.
//!
//! **Phase 4 scope:** this slice ships the primitives + CLI flag + unit
//! tests. Wiring the sandbox into the actual subprocess call sites
//! (halmos / solc / forge / slither) is deferred to V2 — it needs a
//! Linux test environment to tune the bubblewrap bind-mount set against
//! the whole bench corpus before flipping the default. See
//! `docs/book/v2-readiness.md` for the follow-up checklist item.

use std::path::PathBuf;

use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct SandboxConfig {
    /// Project root — granted read+write access inside the sandbox.
    pub project_dir: PathBuf,
    /// Permit network egress. Defaults false. Set true only for
    /// subprocesses that genuinely need network (e.g., `forge install`).
    pub allow_network: bool,
    /// Extra paths granted write access on top of `project_dir`.
    /// Typical entries: `~/.foundry/cache`, `~/.halmos/cache`.
    pub extra_writable: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxKind {
    /// macOS sandbox-exec available (default on macOS hosts).
    MacosSandboxExec,
    /// Linux bubblewrap (`bwrap`) available (default on Linux hosts).
    LinuxBubblewrap,
    /// No sandbox — either an unsupported platform, the operator passed
    /// `--no-sandbox`, or the sandbox binary isn't on PATH. Subprocess
    /// runs unconstrained; one `tracing::warn` fires when this is used
    /// so operators see the gap.
    None,
}

impl SandboxKind {
    /// Detect the platform's preferred sandbox at runtime. Returns
    /// `None` when no supported backend is available; callers can
    /// still construct a [`SandboxKind::None`] explicitly.
    pub fn detect() -> Self {
        if cfg!(target_os = "macos") && which_in_path("sandbox-exec").is_some() {
            return Self::MacosSandboxExec;
        }
        if cfg!(target_os = "linux") && which_in_path("bwrap").is_some() {
            return Self::LinuxBubblewrap;
        }
        Self::None
    }
}

/// Build a [`tokio::process::Command`] that, when spawned, runs
/// `program` inside the chosen sandbox. The caller adds remaining args
/// via `.arg()`/`.args()`; they pass through to the inner program.
///
/// Working directory + environment passed via the returned Command's
/// `.current_dir()` / `.env()` propagate to the inner program (sandbox-exec
/// inherits both; bwrap inherits env, and `--chdir` would be needed for
/// cwd inside the namespace — Phase 4 leaves cwd handling to V2's
/// per-subprocess wiring).
pub fn sandbox_command(program: &str, kind: SandboxKind, cfg: &SandboxConfig) -> Command {
    match kind {
        SandboxKind::MacosSandboxExec => {
            let profile = build_macos_profile(cfg);
            let mut cmd = Command::new("sandbox-exec");
            cmd.arg("-p").arg(profile).arg(program);
            cmd
        }
        SandboxKind::LinuxBubblewrap => {
            let mut cmd = Command::new("bwrap");
            for arg in build_bwrap_args(cfg) {
                cmd.arg(arg);
            }
            cmd.arg("--").arg(program);
            cmd
        }
        SandboxKind::None => {
            tracing::warn!(
                "subprocess `{program}` running without sandbox. Phase 4 \
                 ships sandbox profiles for macOS + Linux but subprocess \
                 wiring defaults to opt-in; V2 picks up default-on once \
                 profiles are tuned against the bench corpus."
            );
            Command::new(program)
        }
    }
}

/// Render the macOS sandbox-exec profile for `cfg`. Exposed so tests
/// can inspect the profile shape.
pub fn build_macos_profile(cfg: &SandboxConfig) -> String {
    let project = escape_scheme_string(&cfg.project_dir.display().to_string());
    let mut writable = String::new();
    writable.push_str(&format!("  (allow file-write* (subpath \"{project}\"))\n"));
    for p in &cfg.extra_writable {
        let path = escape_scheme_string(&p.display().to_string());
        writable.push_str(&format!("  (allow file-write* (subpath \"{path}\"))\n"));
    }
    let network = if cfg.allow_network {
        "  (allow network*)"
    } else {
        "  (deny network*)"
    };
    format!(
        r#"(version 1)
;; Vergil subprocess sandbox profile — Phase 4 Slice B1.
(deny default)
(allow process-fork)
(allow process-exec)
(allow signal (target self))
(allow file-read*)
{writable}  (allow file-write* (subpath "/private/tmp"))
  (allow file-write* (subpath "/private/var/folders"))
  (allow file-write* (subpath "/tmp"))
{network}
(allow mach-lookup)
(allow ipc-posix-shm)
(allow ipc-sysv-shm)
(allow sysctl-read)
"#
    )
}

/// Render the bubblewrap arg list for `cfg`. Exposed so tests can
/// inspect the arg vector.
pub fn build_bwrap_args(cfg: &SandboxConfig) -> Vec<String> {
    let project = cfg.project_dir.display().to_string();
    let mut args: Vec<String> = vec![
        "--ro-bind".into(),
        "/usr".into(),
        "/usr".into(),
        "--ro-bind".into(),
        "/lib".into(),
        "/lib".into(),
        "--ro-bind-try".into(),
        "/lib64".into(),
        "/lib64".into(),
        "--ro-bind-try".into(),
        "/etc/ssl".into(),
        "/etc/ssl".into(),
        "--ro-bind-try".into(),
        "/etc/resolv.conf".into(),
        "/etc/resolv.conf".into(),
        "--proc".into(),
        "/proc".into(),
        "--dev".into(),
        "/dev".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "--bind".into(),
        project.clone(),
        project,
    ];
    if !cfg.allow_network {
        args.push("--unshare-net".into());
    }
    for p in &cfg.extra_writable {
        let s = p.display().to_string();
        args.push("--bind".into());
        args.push(s.clone());
        args.push(s);
    }
    args.push("--die-with-parent".into());
    args
}

/// Tiny PATH walker. Avoids pulling in the `which` crate.
fn which_in_path(bin: &str) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        let cand = dir.join(bin);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Scheme-string escape for the sandbox-exec profile. Backslashes and
/// quotes need to be escaped; everything else is safe. The path here
/// comes from a `PathBuf` so it's already filesystem-valid.
fn escape_scheme_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg_for(project: &str) -> SandboxConfig {
        SandboxConfig {
            project_dir: PathBuf::from(project),
            allow_network: false,
            extra_writable: vec![],
        }
    }

    #[test]
    fn detect_returns_one_of_the_known_kinds_without_panicking() {
        let kind = SandboxKind::detect();
        assert!(matches!(
            kind,
            SandboxKind::MacosSandboxExec | SandboxKind::LinuxBubblewrap | SandboxKind::None
        ));
    }

    #[test]
    fn macos_profile_denies_network_by_default() {
        let p = build_macos_profile(&cfg_for("/proj"));
        assert!(p.contains("(deny default)"), "{p}");
        assert!(p.contains("(deny network*)"), "{p}");
        assert!(p.contains("/proj"), "{p}");
    }

    #[test]
    fn macos_profile_can_enable_network() {
        let mut c = cfg_for("/p");
        c.allow_network = true;
        let p = build_macos_profile(&c);
        assert!(p.contains("(allow network*)"), "{p}");
        assert!(!p.contains("(deny network*)"), "{p}");
    }

    #[test]
    fn macos_profile_includes_extra_writable_paths() {
        let mut c = cfg_for("/p");
        c.extra_writable = vec![PathBuf::from("/cache/foundry")];
        let p = build_macos_profile(&c);
        assert!(p.contains("/cache/foundry"), "{p}");
    }

    #[test]
    fn macos_profile_escapes_quotes_and_backslashes_in_paths() {
        let c = SandboxConfig {
            project_dir: PathBuf::from("/odd\"path"),
            ..Default::default()
        };
        let p = build_macos_profile(&c);
        assert!(
            p.contains("\\\""),
            "expected escaped quote in profile, got: {p}"
        );
    }

    #[test]
    fn bwrap_args_include_default_bindmounts_and_unshare_net() {
        let args = build_bwrap_args(&cfg_for("/proj"));
        let joined = args.join(" ");
        assert!(joined.contains("--ro-bind /usr /usr"), "{joined}");
        assert!(joined.contains("--ro-bind /lib /lib"), "{joined}");
        assert!(joined.contains("--unshare-net"), "{joined}");
        assert!(joined.contains("--die-with-parent"), "{joined}");
        assert!(joined.contains("--bind /proj /proj"), "{joined}");
    }

    #[test]
    fn bwrap_args_drop_unshare_net_when_network_allowed() {
        let mut c = cfg_for("/p");
        c.allow_network = true;
        let args = build_bwrap_args(&c);
        let joined = args.join(" ");
        assert!(!joined.contains("--unshare-net"), "{joined}");
    }

    #[test]
    fn bwrap_args_add_extra_writable_binds() {
        let mut c = cfg_for("/p");
        c.extra_writable = vec![PathBuf::from("/cache/foundry")];
        let args = build_bwrap_args(&c);
        let joined = args.join(" ");
        assert!(
            joined.contains("--bind /cache/foundry /cache/foundry"),
            "{joined}"
        );
    }

    #[test]
    fn sandbox_command_macos_uses_sandbox_exec_program() {
        let cmd = sandbox_command("halmos", SandboxKind::MacosSandboxExec, &cfg_for("/p"));
        let std_cmd = cmd.as_std();
        assert_eq!(std_cmd.get_program().to_string_lossy(), "sandbox-exec");
        let args: Vec<String> = std_cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        // Args: -p, <profile>, halmos
        assert_eq!(args.first().map(|s| s.as_str()), Some("-p"));
        assert!(
            args.iter().any(|a| a == "halmos"),
            "halmos not in args: {args:?}"
        );
    }

    #[test]
    fn sandbox_command_linux_uses_bwrap_program() {
        let cmd = sandbox_command("halmos", SandboxKind::LinuxBubblewrap, &cfg_for("/p"));
        let std_cmd = cmd.as_std();
        assert_eq!(std_cmd.get_program().to_string_lossy(), "bwrap");
        let args: Vec<String> = std_cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        // Args contain --unshare-net and end with -- halmos
        assert!(args.iter().any(|a| a == "--unshare-net"));
        let halmos_idx = args.iter().position(|a| a == "halmos").expect("halmos");
        assert!(halmos_idx > 0, "halmos should be after sandbox args");
        assert_eq!(args.get(halmos_idx - 1).map(|s| s.as_str()), Some("--"));
    }

    #[test]
    fn sandbox_command_none_returns_inner_program_directly() {
        let cmd = sandbox_command("halmos", SandboxKind::None, &cfg_for("/p"));
        let std_cmd = cmd.as_std();
        assert_eq!(std_cmd.get_program().to_string_lossy(), "halmos");
        let args: Vec<String> = std_cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.is_empty(), "expected no sandbox args, got {args:?}");
    }

    #[test]
    fn which_in_path_finds_sh() {
        // /bin/sh exists on every supported host.
        if let Some(p) = which_in_path("sh") {
            assert!(p.ends_with(std::path::Path::new("sh")));
        }
        // If sh isn't on PATH (highly unusual), at least the function
        // returned an Option without panicking.
    }
}
