use vergil_core::portfolio::{Backend, Verdict};

use super::VerifyReport;

pub fn render(report: &VerifyReport) -> String {
    let mut out = String::new();
    out.push_str("# Vergil verification report\n\n");
    out.push_str(&format!("**Project:** `{}`\n\n", report.project));
    out.push_str(&format!(
        "**Properties checked:** {}\n\n",
        report.properties.len()
    ));

    out.push_str("| Property | Verdict | Backend | Wall clock |\n");
    out.push_str("|---|---|---|---|\n");
    for p in &report.properties {
        let (verdict, backend, ms) = match &p.result.verdict {
            Verdict::Verified {
                backend,
                wall_clock_ms,
                ..
            } => ("✓ verified", backend_name(*backend), *wall_clock_ms as i64),
            Verdict::Counterexample {
                backend,
                wall_clock_ms,
                ..
            } => (
                "✗ counterexample",
                backend_name(*backend),
                *wall_clock_ms as i64,
            ),
            Verdict::Unknown { .. } => ("? unknown", "—", -1),
            Verdict::Error { .. } => ("! error", "—", -1),
        };
        let ms_cell = if ms < 0 {
            "—".to_string()
        } else {
            format!("{ms} ms")
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            p.name, verdict, backend, ms_cell
        ));
    }

    out.push_str("\n## Details\n\n");
    for p in &report.properties {
        out.push_str(&format!("### `{}`\n\n", p.name));
        out.push_str(&detail_block(&p.result.verdict));
        out.push('\n');
    }
    out
}

fn detail_block(v: &Verdict) -> String {
    match v {
        Verdict::Verified {
            backend,
            wall_clock_ms,
            smt_query_sha256,
        } => {
            let mut s = format!(
                "Verified by {} in {} ms.\n",
                backend_name(*backend),
                wall_clock_ms
            );
            if let Some(h) = smt_query_sha256 {
                s.push_str(&format!("\nSMT query SHA-256: `{h}`\n"));
            }
            s
        }
        Verdict::Counterexample {
            backend,
            property,
            message,
            wall_clock_ms,
        } => format!(
            "Counterexample from {} ({} ms) for `{}`:\n\n```\n{}\n```\n",
            backend_name(*backend),
            wall_clock_ms,
            property,
            message
        ),
        Verdict::Unknown { backends } => {
            let mut s = String::from("Neither backend reached a verdict.\n\n");
            for o in backends {
                s.push_str(&format!(
                    "- **{}** ({:?}, {} ms): {}\n",
                    backend_name(o.backend),
                    o.state,
                    o.wall_clock_ms,
                    o.detail
                ));
            }
            s
        }
        Verdict::Error { backends } => {
            let mut s = String::from("Both backends errored:\n\n");
            for o in backends {
                s.push_str(&format!(
                    "- **{}**: {}\n",
                    backend_name(o.backend),
                    o.detail
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
