pub mod markdown;
pub mod text;

use serde::Serialize;
use vergil_core::portfolio::{PortfolioResult, Verdict};

#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub project: String,
    pub properties: Vec<PropertyOutcome>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PropertyOutcome {
    pub name: String,
    pub result: PortfolioResult,
}

impl VerifyReport {
    /// Overall exit code per SPEC §3.1: 0 verified, 1 cex, 2 unknown, 3 error.
    pub fn exit_code(&self) -> u8 {
        let mut has_cex = false;
        let mut has_unknown = false;
        let mut has_error = false;
        for p in &self.properties {
            match &p.result.verdict {
                Verdict::Verified { .. } => {}
                Verdict::Counterexample { .. } => has_cex = true,
                Verdict::Unknown { .. } => has_unknown = true,
                Verdict::Error { .. } => has_error = true,
            }
        }
        if has_cex {
            1
        } else if has_error {
            3
        } else if has_unknown {
            2
        } else {
            0
        }
    }
}
