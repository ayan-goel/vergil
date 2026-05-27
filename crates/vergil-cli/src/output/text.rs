use vergil_core::portfolio::{Backend, BackendState, Verdict};

use super::VerifyReport;

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const RESET: &str = "\x1b[0m";

pub fn render(report: &VerifyReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("Vergil verify — project: {}\n", report.project));
    out.push_str(&format!("{} properties\n\n", report.properties.len()));

    for p in &report.properties {
        out.push_str(&render_one(p));
    }

    // Summary line
    let mut verified = 0;
    let mut cex = 0;
    let mut unknown = 0;
    let mut error = 0;
    for p in &report.properties {
        match &p.result.verdict {
            Verdict::Verified { .. } => verified += 1,
            Verdict::Counterexample { .. } => cex += 1,
            Verdict::Unknown { .. } => unknown += 1,
            Verdict::Error { .. } => error += 1,
        }
    }
    out.push_str(&format!(
        "\nSummary: {GREEN}{verified} verified{RESET}, {RED}{cex} counterexample{RESET}, {YELLOW}{unknown} unknown{RESET}, {error} error\n"
    ));
    out
}

fn render_one(p: &super::PropertyOutcome) -> String {
    match &p.result.verdict {
        Verdict::Verified {
            backend,
            wall_clock_ms,
            ..
        } => format!(
            "  {GREEN}✓{RESET} {} — verified by {} in {}ms\n",
            p.name,
            backend_name(*backend),
            wall_clock_ms
        ),
        Verdict::Counterexample {
            backend,
            property: _,
            message,
            wall_clock_ms,
        } => format!(
            "  {RED}✗{RESET} {} — counterexample from {} ({}ms): {}\n",
            p.name,
            backend_name(*backend),
            wall_clock_ms,
            truncate(message, 200)
        ),
        Verdict::Unknown { backends } => {
            let mut s = format!("  {YELLOW}?{RESET} {} — unknown\n", p.name);
            for o in backends {
                s.push_str(&format!(
                    "      {} → {}: {}\n",
                    backend_name(o.backend),
                    state_name(o.state),
                    truncate(&o.detail, 160)
                ));
            }
            s
        }
        Verdict::Error { backends } => {
            let mut s = format!("  ! {} — error\n", p.name);
            for o in backends {
                s.push_str(&format!(
                    "      {}: {}\n",
                    backend_name(o.backend),
                    truncate(&o.detail, 160)
                ));
            }
            s
        }
    }
}

fn backend_name(b: Backend) -> &'static str {
    match b {
        Backend::Halmos => "halmos",
        Backend::SmtChecker => "smtchecker",
    }
}

fn state_name(s: BackendState) -> &'static str {
    match s {
        BackendState::Verified => "verified",
        BackendState::Counterexample => "counterexample",
        BackendState::Unknown => "unknown",
        BackendState::Timeout => "timeout",
        BackendState::Error => "error",
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.replace('\n', " ")
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}
