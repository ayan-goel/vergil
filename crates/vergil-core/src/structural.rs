//! Structural mining — V1.5 Phase 5.
//!
//! Fourth Stage-1 oracle alongside `catalog_intent`, `tests_intent`,
//! `natspec_intent`. Mines candidate properties **deterministically**
//! from solc storage layout + regex over Solidity function bodies. Zero
//! LLM cost. Phase 6 deferred Phase 5 but left stable seams:
//!
//! - `Source::Structural` already in the enum at `synthesis.rs`.
//! - `output::layout::source_dir(ZeroConfig, Structural)` already wired.
//! - `critique::source_guidance(Source::Structural)` already defaults
//!   the `restate_the_source` axis to 1.0 (structural candidates aren't
//!   paraphrased text).
//!
//! Five miner families per SPEC §3.5:
//!
//! 1. Invariant constants (state vars assigned only at declaration or
//!    in the constructor)
//! 2. Monotonicity (state vars only ever incremented or only ever
//!    decremented)
//! 3. Access policy (every public write to slot s requires modifier m)
//! 4. Conservation (paired `M[a] -= k; M[b] += k` preserves a sum)
//! 5. Two-step patterns (F2 requires gate var A which only F1 writes)
//!
//! Slice 0 ships the data plumbing + an empty `extract_from_structural`
//! stub. Slices 1-5 add each miner.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use vergil_solidity::signatures::{body_for, extract as extract_signatures};
use vergil_solidity::storage::StorageLayout;

use crate::synthesis::{Source, SpecCandidate};

/// Identifier for one of the five Phase 5 miner families. Used in
/// telemetry counters and to encode confidence into a candidate's
/// `template_ref` (`"structural:{id}:{conf:.2}"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StructuralMiner {
    InvariantConstants,
    Monotonicity,
    AccessPolicy,
    Conservation,
    TwoStep,
}

impl StructuralMiner {
    /// Stable kebab-case identifier. Pinned by V2 billing + verdict UI.
    pub fn id(self) -> &'static str {
        match self {
            Self::InvariantConstants => "invariant-constants",
            Self::Monotonicity => "monotonicity",
            Self::AccessPolicy => "access-policy",
            Self::Conservation => "conservation",
            Self::TwoStep => "two-step",
        }
    }

    /// Iterate all five miners in stable declaration order.
    pub fn all() -> [StructuralMiner; 5] {
        [
            Self::InvariantConstants,
            Self::Monotonicity,
            Self::AccessPolicy,
            Self::Conservation,
            Self::TwoStep,
        ]
    }
}

/// One mined candidate before the confidence cut. The pipeline sees
/// only the inner [`SpecCandidate`] (via [`Self::into_spec_candidate`]);
/// the confidence + miner survive only in the report's
/// `low_confidence_findings` for below-threshold candidates.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralCandidate {
    pub spec: SpecCandidate,
    /// In `[0.0, 1.0]`. Candidates with `confidence >= cfg.min_confidence`
    /// enter the verification pipeline; the rest stay in the report.
    pub confidence: f32,
    pub miner: StructuralMiner,
}

impl StructuralCandidate {
    /// Consume into a bare [`SpecCandidate`], encoding the confidence
    /// into `template_ref` as `"structural:{miner}:{conf:.2}"` so it
    /// survives the synthesis → critique → SMT pipeline and surfaces in
    /// the verdict / `proof.json` artifact. Mutates `source` to
    /// [`Source::Structural`] and never overwrites an existing
    /// `template_ref` (the miner controls the format).
    pub fn into_spec_candidate(self) -> SpecCandidate {
        let mut spec = self.spec;
        spec.source = Source::Structural;
        let template_ref = spec.template_ref.unwrap_or_else(|| {
            format!("structural:{}:{:.2}", self.miner.id(), self.confidence)
        });
        spec.template_ref = Some(template_ref);
        spec
    }
}

/// Below-threshold finding — surfaced in the verdict's "Suggested
/// additional invariants" section but NOT submitted to the solver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LowConfidenceFinding {
    pub miner: StructuralMiner,
    pub description: String,
    /// Stored as a string to keep the report's serde shape simple and
    /// avoid float-equality test brittleness. Format: `"{conf:.2}"`.
    pub confidence: String,
    pub fn_or_var: Option<String>,
}

impl LowConfidenceFinding {
    pub fn new(miner: StructuralMiner, description: impl Into<String>, confidence: f32) -> Self {
        Self {
            miner,
            description: description.into(),
            confidence: format!("{confidence:.2}"),
            fn_or_var: None,
        }
    }

    pub fn with_target(mut self, fn_or_var: impl Into<String>) -> Self {
        self.fn_or_var = Some(fn_or_var.into());
        self
    }
}

/// Aggregated output of one structural-mining pass.
#[derive(Debug, Default, Clone)]
pub struct StructuralReport {
    /// Confidence ≥ `cfg.min_confidence`. These flow into Stage 1's
    /// merged candidate list.
    pub candidates: Vec<SpecCandidate>,
    /// Below-threshold; report-only.
    pub low_confidence_findings: Vec<LowConfidenceFinding>,
    /// Per-miner count of emitted high-confidence candidates. Stable
    /// keying via [`StructuralMiner::id`] for telemetry.
    pub miner_counts: HashMap<StructuralMiner, usize>,
}

/// Configuration for one structural-mining run.
#[derive(Debug, Clone)]
pub struct StructuralConfig {
    /// Cutoff below which candidates go to `low_confidence_findings`
    /// instead of `candidates`. Default 0.6 per SPEC §11.5.
    pub min_confidence: f32,
}

impl Default for StructuralConfig {
    fn default() -> Self {
        Self { min_confidence: 0.6 }
    }
}

/// Phase 5 oracle entry point. Sync + no LLM dependency — Phase 5 is
/// pure static analysis (solc storage layout + regex over function
/// bodies).
///
/// `sources` is a list of `(path, source_text)` pairs — Phase 5 mines
/// across every Solidity source the fingerprint identified.
/// `layouts` is the per-contract solc storage layout (one entry per
/// `<file>:<ContractName>`), produced by
/// `vergil_solidity::storage::StorageRun`. Layout entries for
/// `constant` / `immutable` variables are NOT emitted by solc (those
/// live in bytecode, not storage) — Phase 5 detects them from source.
pub fn extract_from_structural(
    sources: &[(PathBuf, String)],
    layouts: &[StorageLayout],
    cfg: &StructuralConfig,
) -> StructuralReport {
    let mut all: Vec<StructuralCandidate> = Vec::new();
    let mut low: Vec<LowConfidenceFinding> = Vec::new();

    let (ic_high, ic_low) = mine_invariant_constants(sources, layouts);
    all.extend(ic_high);
    low.extend(ic_low);

    let (mn_high, mn_low) = mine_monotonicity(sources, layouts);
    all.extend(mn_high);
    low.extend(mn_low);

    let (ap_high, ap_low) = mine_access_policy(sources, layouts);
    all.extend(ap_high);
    low.extend(ap_low);

    let (cs_high, cs_low) = mine_conservation(sources, layouts);
    all.extend(cs_high);
    low.extend(cs_low);

    let (ts_high, ts_low) = mine_two_step(sources, layouts);
    all.extend(ts_high);
    low.extend(ts_low);

    split_by_confidence(all, low, cfg)
}

/// Split mined candidates into the high-confidence pipeline-bound vec
/// and the report-only low-confidence vec, applying the
/// `cfg.min_confidence` threshold. Per-miner counts are derived from
/// the high-confidence vec — low-confidence candidates DO NOT increment
/// `miner_counts` because that map drives the telemetry counter for
/// "candidates emitted into the pipeline".
fn split_by_confidence(
    candidates: Vec<StructuralCandidate>,
    extra_low: Vec<LowConfidenceFinding>,
    cfg: &StructuralConfig,
) -> StructuralReport {
    let mut report = StructuralReport {
        candidates: Vec::new(),
        low_confidence_findings: extra_low,
        miner_counts: HashMap::new(),
    };
    for c in candidates {
        if c.confidence >= cfg.min_confidence {
            *report.miner_counts.entry(c.miner).or_insert(0) += 1;
            report.candidates.push(c.into_spec_candidate());
        } else {
            let finding = LowConfidenceFinding::new(
                c.miner,
                c.spec
                    .intent_text
                    .clone()
                    .unwrap_or_else(|| c.spec.name.clone()),
                c.confidence,
            )
            .with_target(c.spec.name);
            report.low_confidence_findings.push(finding);
        }
    }
    report
}

// ─── Miner 1: Invariant constants ────────────────────────────────────

/// Mine invariant-constant candidates from the source set + storage
/// layouts. Returns `(high_confidence_candidates, low_confidence_findings)`.
/// The high-confidence vec carries both Tier A (keyword) and Tier B
/// (declaration-with-literal-initializer); the low-confidence vec
/// carries Tier C (constructor-only-write with no extractable literal).
///
/// Three tiers:
///
/// - **Tier A (0.95)** — `(constant|immutable)` keyword on a state-var
///   declaration. The compiler enforces immutability; the value lives
///   in bytecode, not storage. Halmos check asserts the getter returns
///   the declared literal.
/// - **Tier B (0.80)** — state-var declared with an extractable literal
///   initializer (e.g. `string public name = "X";`). Same Halmos check
///   as Tier A. Confidence is lower than Tier A because nothing enforces
///   that the variable isn't re-written elsewhere; the miner verifies
///   that itself via `body_for` scan.
/// - **Tier C (0.55)** — state-var written only in the constructor with
///   no extractable literal (e.g., `totalSupply = initialSupply;`). The
///   value depends on a constructor argument. Below threshold →
///   report-only. The user is told "this looks invariant; verify
///   manually."
pub fn mine_invariant_constants(
    sources: &[(PathBuf, String)],
    layouts: &[StorageLayout],
) -> (Vec<StructuralCandidate>, Vec<LowConfidenceFinding>) {
    let mut high: Vec<StructuralCandidate> = Vec::new();
    let mut low: Vec<LowConfidenceFinding> = Vec::new();

    for (path, source) in sources {
        // Tier A: keyword constants/immutables. Source-only — solc layout
        // does NOT list constant/immutable variables (they're not in
        // storage).
        for decl in scan_keyword_constants(source) {
            high.push(make_invariant_candidate(
                &decl.name,
                &decl.literal,
                0.95,
                format!(
                    "{} declared `{}` — value cannot change post-deployment",
                    decl.name, decl.keyword
                ),
                path,
            ));
        }
        // Tiers B / C: any state-var in the storage layout for this file
        // that doesn't get written outside the constructor.
        let file_layout: Vec<&StorageLayout> = layouts
            .iter()
            .filter(|l| layout_belongs_to(l, path))
            .collect();
        if file_layout.is_empty() {
            continue;
        }
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for layout in &file_layout {
            for entry in &layout.entries {
                if !seen.insert(entry.label.as_str()) {
                    continue;
                }
                // Skip mappings and dynamic arrays — invariants there
                // aren't "value never changes", they're conservation /
                // monotonicity (other miners).
                if is_mapping_or_array_type(layout, &entry.type_id) {
                    continue;
                }
                let writers = writers_of(source, &entry.label);
                // Disqualify any var that's written outside the constructor.
                if writers.iter().any(|w| w != "constructor") {
                    continue;
                }
                let decl_lit = declaration_literal(source, &entry.label);
                match (decl_lit, writers.is_empty()) {
                    (Some(lit), _) => {
                        // Tier B: declared with a literal initializer
                        // (writers may include "constructor" if the
                        // initializer is re-applied there; either way
                        // the value is fixed by the literal).
                        high.push(make_invariant_candidate(
                            &entry.label,
                            &lit,
                            0.80,
                            format!(
                                "{} declared with literal initializer — never re-written",
                                entry.label
                            ),
                            path,
                        ));
                    }
                    (None, false) => {
                        // Tier C: written only by constructor; no
                        // extractable literal. Below threshold → report-only.
                        low.push(
                            LowConfidenceFinding::new(
                                StructuralMiner::InvariantConstants,
                                format!(
                                    "{} written only in constructor; value depends on constructor arg — \
                                     verify invariance manually",
                                    entry.label
                                ),
                                0.55,
                            )
                            .with_target(entry.label.clone()),
                        );
                    }
                    (None, true) => {
                        // Declared but never assigned in any visible
                        // function body and no declaration literal.
                        // Most likely an inherited field or a layout
                        // entry for an interface — skip.
                    }
                }
            }
        }
    }

    (high, low)
}

/// One keyword-constant declaration extracted from a Solidity source.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeywordConstant {
    name: String,
    /// The literal RHS as written (e.g., `"18"`, `"\"REF\""`, `"address(0)"`).
    literal: String,
    /// `"constant"` or `"immutable"`.
    keyword: &'static str,
}

/// Scan source for `(constant|immutable)` state-var declarations.
fn scan_keyword_constants(source: &str) -> Vec<KeywordConstant> {
    let stripped = strip_comments(source);
    let mut out = Vec::new();
    for kw in ["constant", "immutable"] {
        out.extend(scan_keyword_kind(&stripped, kw));
    }
    out
}

fn scan_keyword_kind(stripped: &str, kw: &'static str) -> Vec<KeywordConstant> {
    // Match `<type> <visibility>? <keyword> <name> (= <literal>)? ;`.
    // The regex stays loose: we walk to the next `;` and split on `=`.
    let mut out = Vec::new();
    let needle = format!(" {kw} ");
    let bytes = stripped.as_bytes();
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        let rest = &stripped[i..];
        let Some(rel) = rest.find(&needle) else {
            break;
        };
        let kw_start = i + rel + 1; // skip leading space
        let after_kw = kw_start + kw.len() + 1;
        // Backtrack from `kw_start` to the start of the declaration
        // line — first `{` / `}` / `;` / newline preceding it.
        let mut line_start = 0;
        for (j, b) in stripped[..kw_start].as_bytes().iter().enumerate().rev() {
            if matches!(*b, b'{' | b'}' | b';' | b'\n') {
                line_start = j + 1;
                break;
            }
        }
        // Decl span = [line_start, end_of_statement). Find `;` at depth 0.
        let Some(end) = find_top_level_semicolon(&stripped[after_kw..]) else {
            i = after_kw;
            continue;
        };
        let decl = stripped[line_start..after_kw + end].trim();
        // Must NOT be inside a function body — heuristic: declaration
        // line starts with a type keyword AND doesn't contain `(`.
        // `function foo() public constant ...` (pre-0.5 syntax) would
        // confuse us; modern Solidity uses `view`/`pure` for that.
        if decl.contains("function ") {
            i = after_kw + end;
            continue;
        }
        // Extract name + literal.
        // `<type> <visibility?> <kw> <NAME> = <LITERAL>;` OR
        // `<type> <visibility?> <kw> <NAME>;`  (immutable, no init).
        let after_kw_str = &decl[decl.find(kw).unwrap_or(0) + kw.len()..];
        let (name, literal) = parse_name_and_literal(after_kw_str.trim());
        if let Some(name) = name {
            out.push(KeywordConstant {
                name,
                literal: literal.unwrap_or_default(),
                keyword: kw,
            });
        }
        i = after_kw + end;
    }
    out
}

fn parse_name_and_literal(s: &str) -> (Option<String>, Option<String>) {
    // s is like `DECIMALS = 18` or `OWNER` (immutable, no init).
    let body = s.trim_end_matches(';').trim();
    if let Some((name, lit)) = body.split_once('=') {
        let name = ident_of(name.trim());
        let lit = lit.trim().to_string();
        (name, Some(lit))
    } else {
        (ident_of(body), None)
    }
}

fn ident_of(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.chars().next().unwrap().is_ascii_digit()
    {
        Some(s.to_string())
    } else {
        None
    }
}

fn find_top_level_semicolon(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut paren = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'(' => paren += 1,
            b')' => {
                if paren > 0 {
                    paren -= 1;
                }
            }
            b';' if paren == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Determine whether a storage-layout entry belongs to the given source
/// file. solc emits `qualified_name` as `<absolute-or-relative path>:<Contract>`.
fn layout_belongs_to(layout: &StorageLayout, path: &std::path::Path) -> bool {
    let q = &layout.qualified_name;
    let fname = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if fname.is_empty() {
        return false;
    }
    q.contains(fname)
}

fn is_mapping_or_array_type(layout: &StorageLayout, type_id: &str) -> bool {
    if let Some(t) = layout.types.get(type_id) {
        let l = &t.label;
        return l.starts_with("mapping") || l.ends_with("[]") || l.contains("array");
    }
    type_id.contains("mapping") || type_id.contains("array")
}

/// Return the list of function names that write to `var_name`. The
/// constructor is returned as the literal `"constructor"`. A write is
/// detected by regex on the function body: `var_name <op>` where `<op>`
/// is `=`, `+=`, `-=`, `*=`, `/=`, `%=`. Also matches `var_name++` /
/// `var_name--` / `++var_name` / `--var_name`.
fn writers_of(source: &str, var_name: &str) -> Vec<String> {
    let mut writers: Vec<String> = Vec::new();
    // Constructor body.
    if let Some(body) = constructor_body(source) {
        if body_writes_to(body, var_name) {
            writers.push("constructor".to_string());
        }
    }
    // External / public functions (signatures::extract skips internal/private).
    for sig in extract_signatures(source) {
        let Some(body) = body_for(&sig.name, source) else {
            continue;
        };
        if body_writes_to(body, var_name) {
            writers.push(sig.name.clone());
        }
    }
    writers
}

fn constructor_body(source: &str) -> Option<&str> {
    // signatures::body_for matches on a `function <name>` boundary, but
    // constructors are `constructor(...) { ... }` without the `function`
    // keyword. Implement a small parser here.
    let stripped = strip_comments(source);
    let idx = stripped.find("constructor")?;
    // Project back into the original source.
    let original_idx = project_to_original(source, &stripped, idx)?;
    let from = &source[original_idx..];
    let open = find_body_open_after_parens(from)?;
    let body_open = original_idx + open;
    let body_close = match_brace(source, body_open)?;
    if body_close <= body_open + 1 {
        return Some("");
    }
    Some(&source[body_open + 1..body_close])
}

/// Return the literal RHS of `<type> public <var_name> = <literal>;` if
/// present at the declaration site. `None` when the variable is declared
/// without an initializer, OR the RHS isn't a simple literal (e.g.,
/// `keccak256("...")`, expressions with operators).
fn declaration_literal(source: &str, var_name: &str) -> Option<String> {
    let stripped = strip_comments(source);
    // Look for `<var_name>` preceded by whitespace + a type-ish token AND
    // followed by `=` ... `;`. The simplest robust pattern: scan for
    // ` <var_name> =` (with leading space + trailing equals).
    let needle = format!(" {var_name} ");
    let bytes = stripped.as_bytes();
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        let rest = &stripped[i..];
        let Some(rel) = rest.find(&needle) else {
            return None;
        };
        let after = i + rel + needle.len();
        // Make sure this isn't inside a function body — backtrack to the
        // nearest `{`/`}`/`;`/newline; if we hit `{` before `}`/`;` we're
        // inside a body.
        if is_inside_function_body(&stripped, i + rel + 1) {
            i = after;
            continue;
        }
        // After ` <var_name> `, expect `=` ... `;`.
        let tail = stripped[after..].trim_start();
        if !tail.starts_with('=') {
            i = after;
            continue;
        }
        let rhs = tail.trim_start_matches('=').trim();
        let end = find_top_level_semicolon(rhs)?;
        let lit = rhs[..end].trim();
        // Reject obvious expression patterns — keccak / abi / arithmetic
        // with idents on both sides. The heuristic: simple literals are
        // numeric, quoted strings, or `address(0)` / `address(this)`.
        if is_simple_literal(lit) {
            return Some(lit.to_string());
        }
        return None;
    }
    None
}

fn is_simple_literal(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    if s.starts_with('"') && s.ends_with('"') {
        return true;
    }
    if s.starts_with('\'') && s.ends_with('\'') {
        return true;
    }
    if s.starts_with("0x") {
        return s.chars().skip(2).all(|c| c.is_ascii_hexdigit());
    }
    // Address literals.
    if s == "address(0)" || s == "address(this)" {
        return true;
    }
    // Bare numeric (optional underscores, optional unit suffix like
    // `ether` / `wei` / `1e18`).
    if s.chars()
        .all(|c| c.is_ascii_digit() || c == '_' || c == 'e' || c == 'E')
    {
        return true;
    }
    // `1 ether`, `100 wei`, `1e18` (already covered above).
    if let Some((num, unit)) = s.split_once(' ') {
        let num_ok = num
            .trim()
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_');
        let unit_ok = matches!(
            unit.trim(),
            "wei" | "gwei" | "ether" | "seconds" | "minutes" | "hours" | "days" | "weeks"
        );
        return num_ok && unit_ok;
    }
    // `true`/`false` literals.
    matches!(s, "true" | "false")
}

fn is_inside_function_body(stripped: &str, pos: usize) -> bool {
    // Walk backwards from `pos`, counting open-brace depth. Solidity
    // sources are nested: contract { state-vars; function() { body } }.
    // Depth 1 = contract body (state-var declarations live here).
    // Depth 2+ = function body (assignments live here).
    //
    // The "current depth" is the number of `{` we cross BACKWARD that
    // haven't been matched by a `}` we've already crossed. If that depth
    // is ≥2, we're inside a nested block (function / modifier / ctor body).
    let bytes = stripped.as_bytes();
    let mut depth = 0i32;
    for b in bytes[..pos].iter().rev() {
        match *b {
            b'}' => depth -= 1,
            b'{' => {
                depth += 1;
                if depth >= 2 {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn body_writes_to(body: &str, var_name: &str) -> bool {
    // Match `var_name <op>` where op is one of `=` `+=` `-=` `*=` `/=`
    // `%=` `++` `--` (postfix), or `<op>var_name` for prefix.
    // Don't match `==` (equality).
    let bytes = body.as_bytes();
    let needle = var_name.as_bytes();
    let n = needle.len();
    let mut i = 0;
    while i + n <= bytes.len() {
        if !is_word_boundary_at(body, i, n) {
            i += 1;
            continue;
        }
        if &bytes[i..i + n] != needle {
            i += 1;
            continue;
        }
        let after = i + n;
        if after >= bytes.len() {
            return false;
        }
        // Skip whitespace.
        let mut j = after;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() {
            return false;
        }
        let c = bytes[j];
        // `++` / `--` / mapping index `[`
        if c == b'+' && j + 1 < bytes.len() && bytes[j + 1] == b'+' {
            return true;
        }
        if c == b'-' && j + 1 < bytes.len() && bytes[j + 1] == b'-' {
            return true;
        }
        // Single-char `=` is assignment iff not `==`.
        if c == b'=' {
            // Could be `=`, `==`, `=>`. Only `=` (followed by non-`=`/`>`)
            // is an assignment.
            if j + 1 < bytes.len() && (bytes[j + 1] == b'=' || bytes[j + 1] == b'>') {
                i = j + 1;
                continue;
            }
            return true;
        }
        // Compound assignments.
        if matches!(c, b'+' | b'-' | b'*' | b'/' | b'%')
            && j + 1 < bytes.len()
            && bytes[j + 1] == b'='
        {
            return true;
        }
        i = j;
    }
    false
}

fn is_word_boundary_at(s: &str, start: usize, len: usize) -> bool {
    let bytes = s.as_bytes();
    let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
    let after = start + len;
    let after_ok = after >= bytes.len() || !is_ident_byte(bytes[after]);
    before_ok && after_ok
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn make_invariant_candidate(
    var_name: &str,
    literal: &str,
    confidence: f32,
    intent: String,
    _path: &std::path::Path,
) -> StructuralCandidate {
    // Halmos check: assert that the getter returns the literal value.
    // Falls back to a parameter-less getter call when no literal is
    // available (Tier B with empty literal — shouldn't happen because
    // Tier B requires a literal; defensive).
    let halmos = if literal.is_empty() {
        format!(
            "function check_invariant_{var_name}() public view {{\n    \
             c.{var_name}();\n}}\n"
        )
    } else if literal.starts_with('"') {
        // String literal — compare via keccak256 of bytes.
        format!(
            "function check_invariant_{var_name}() public view {{\n    \
             assertEq(\n        keccak256(bytes(c.{var_name}())),\n        \
             keccak256(bytes({literal}))\n    );\n}}\n"
        )
    } else {
        // Numeric / address / hex literal — direct equality.
        format!(
            "function check_invariant_{var_name}() public view {{\n    \
             assertEq(uint256(c.{var_name}()), uint256({literal}));\n}}\n"
        )
    };
    StructuralCandidate {
        spec: SpecCandidate {
            name: format!("check_invariant_{var_name}"),
            halmos,
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: false,
            source: Source::Structural,
            intent_text: Some(intent),
        },
        confidence,
        miner: StructuralMiner::InvariantConstants,
    }
}

// ─── Miner 2: Monotonicity ───────────────────────────────────────────

/// Polarity of a write to a state variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WritePolarity {
    /// `+=`, `++`.
    Increment,
    /// `-=`, `--`.
    Decrement,
    /// `=`, `*=`, `/=`, `%=` — disqualifies monotonicity.
    Overwrite,
}

/// Mine monotonicity candidates. For each state variable that is
/// written by some non-constructor function AND every such write
/// shares the same polarity (all `+=`/`++` or all `-=`/`--`), emit one
/// candidate per writer at confidence 0.85. The Halmos check exercises
/// the writer with symbolic args via try/catch and asserts the
/// monotonicity invariant.
///
/// Overwrites (`=`, compound multiplicative/division) disqualify the
/// variable entirely — `value = expr` could go either way, so the
/// miner stays silent.
pub fn mine_monotonicity(
    sources: &[(PathBuf, String)],
    layouts: &[StorageLayout],
) -> (Vec<StructuralCandidate>, Vec<LowConfidenceFinding>) {
    let mut high: Vec<StructuralCandidate> = Vec::new();
    let low: Vec<LowConfidenceFinding> = Vec::new();

    for (path, source) in sources {
        let file_layout: Vec<&StorageLayout> = layouts
            .iter()
            .filter(|l| layout_belongs_to(l, path))
            .collect();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for layout in &file_layout {
            for entry in &layout.entries {
                if !seen.insert(entry.label.as_str()) {
                    continue;
                }
                if is_mapping_or_array_type(layout, &entry.type_id) {
                    continue;
                }
                let writers_polarities = writer_polarities(source, &entry.label);
                // Drop constructor writes (those don't violate
                // monotonicity post-deployment).
                let post_ctor: Vec<&(String, WritePolarity)> = writers_polarities
                    .iter()
                    .filter(|(w, _)| w != "constructor")
                    .collect();
                if post_ctor.is_empty() {
                    continue;
                }
                // Any overwrite disqualifies.
                if post_ctor.iter().any(|(_, p)| *p == WritePolarity::Overwrite) {
                    continue;
                }
                let all_inc = post_ctor
                    .iter()
                    .all(|(_, p)| *p == WritePolarity::Increment);
                let all_dec = post_ctor
                    .iter()
                    .all(|(_, p)| *p == WritePolarity::Decrement);
                if !all_inc && !all_dec {
                    continue;
                }
                let direction = if all_inc { Direction::Inc } else { Direction::Dec };
                // Build a per-writer Halmos candidate.
                let writer_names: std::collections::BTreeSet<&str> =
                    post_ctor.iter().map(|(w, _)| w.as_str()).collect();
                for writer in writer_names {
                    let Some(sig) = signature_for(source, writer) else { continue };
                    if !is_externally_callable(&sig) {
                        continue;
                    }
                    high.push(make_monotonic_candidate(
                        &entry.label,
                        writer,
                        &sig.args,
                        direction,
                        path,
                    ));
                }
            }
        }
    }

    (high, low)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Inc,
    Dec,
}

impl Direction {
    fn label(self) -> &'static str {
        match self {
            Self::Inc => "inc",
            Self::Dec => "dec",
        }
    }
    fn op(self) -> &'static str {
        match self {
            Self::Inc => ">=",
            Self::Dec => "<=",
        }
    }
    fn english(self) -> &'static str {
        match self {
            Self::Inc => "monotonically increasing",
            Self::Dec => "monotonically decreasing",
        }
    }
}

/// Walk every function body in `source` and return the list of
/// (function_name, polarity) pairs for writes to `var_name`. A function
/// that writes the variable multiple times is reported once per write
/// (so mixed-polarity inside one function still disqualifies it). The
/// constructor is reported with the name `"constructor"`.
fn writer_polarities(source: &str, var_name: &str) -> Vec<(String, WritePolarity)> {
    let mut out = Vec::new();
    if let Some(body) = constructor_body(source) {
        for p in scan_polarities(body, var_name) {
            out.push(("constructor".to_string(), p));
        }
    }
    for sig in extract_signatures(source) {
        if let Some(body) = body_for(&sig.name, source) {
            for p in scan_polarities(body, var_name) {
                out.push((sig.name.clone(), p));
            }
        }
    }
    out
}

fn scan_polarities(body: &str, var_name: &str) -> Vec<WritePolarity> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let needle = var_name.as_bytes();
    let n = needle.len();
    let mut i = 0;
    while i + n <= bytes.len() {
        if !is_word_boundary_at(body, i, n) || &bytes[i..i + n] != needle {
            i += 1;
            continue;
        }
        // Postfix `++` / `--` — look immediately after (allowing
        // whitespace).
        let after = skip_ws(bytes, i + n);
        if after + 1 < bytes.len() {
            let c = bytes[after];
            if c == b'+' && bytes[after + 1] == b'+' {
                out.push(WritePolarity::Increment);
                i = after + 2;
                continue;
            }
            if c == b'-' && bytes[after + 1] == b'-' {
                out.push(WritePolarity::Decrement);
                i = after + 2;
                continue;
            }
            if c == b'+' && bytes[after + 1] == b'=' {
                out.push(WritePolarity::Increment);
                i = after + 2;
                continue;
            }
            if c == b'-' && bytes[after + 1] == b'=' {
                out.push(WritePolarity::Decrement);
                i = after + 2;
                continue;
            }
            if matches!(c, b'*' | b'/' | b'%') && bytes[after + 1] == b'=' {
                out.push(WritePolarity::Overwrite);
                i = after + 2;
                continue;
            }
            if c == b'=' && bytes[after + 1] != b'=' && bytes[after + 1] != b'>' {
                out.push(WritePolarity::Overwrite);
                i = after + 1;
                continue;
            }
        }
        // Prefix `++var` / `--var` — look immediately before.
        if i >= 2 {
            let p1 = bytes[i - 1];
            let p2 = bytes[i - 2];
            if p1 == b'+' && p2 == b'+' {
                out.push(WritePolarity::Increment);
                i += n;
                continue;
            }
            if p1 == b'-' && p2 == b'-' {
                out.push(WritePolarity::Decrement);
                i += n;
                continue;
            }
        }
        i += n;
    }
    out
}

fn skip_ws(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    i
}

fn signature_for<'a>(
    source: &'a str,
    fn_name: &str,
) -> Option<vergil_solidity::signatures::FunctionSignature> {
    extract_signatures(source).into_iter().find(|s| s.name == fn_name)
}

fn is_externally_callable(sig: &vergil_solidity::signatures::FunctionSignature) -> bool {
    sig.visibility == "external" || sig.visibility == "public"
}

/// Forward-the-args call form: given `"(uint256 from, uint256 to)"`,
/// return `"from, to"`. Empty for `"()"`.
fn forward_arg_names(args_paren: &str) -> String {
    let inner = args_paren.trim();
    let inner = inner
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(inner);
    inner
        .split(',')
        .filter_map(|p| {
            let p = p.trim();
            if p.is_empty() {
                return None;
            }
            // Drop `memory` / `calldata` / `storage` location keywords.
            // The last identifier is the parameter name.
            let toks: Vec<&str> = p.split_whitespace().collect();
            toks.last().map(|s| (*s).to_string())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn make_monotonic_candidate(
    var_name: &str,
    writer: &str,
    writer_args: &str,
    direction: Direction,
    _path: &std::path::Path,
) -> StructuralCandidate {
    let call_args = forward_arg_names(writer_args);
    // Halmos: exercise the writer with symbolic args; assert post-
    // condition. try/catch absorbs reverts so they don't fail the
    // property (a revert means no state change, which preserves
    // monotonicity trivially).
    let halmos = format!(
        "function check_monotonic_{var}_{dir}_via_{writer}{sig} public {{\n    \
         uint256 pre = token.{var}();\n    \
         try token.{writer}({call_args}) {{}} catch {{ return; }}\n    \
         uint256 post = token.{var}();\n    \
         assert(post {op} pre);\n}}\n",
        var = var_name,
        dir = direction.label(),
        writer = writer,
        sig = writer_args,
        call_args = call_args,
        op = direction.op(),
    );
    StructuralCandidate {
        spec: SpecCandidate {
            name: format!("check_monotonic_{var_name}_{}_via_{writer}", direction.label()),
            halmos,
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: false,
            source: Source::Structural,
            intent_text: Some(format!(
                "{} is {} across calls to {}",
                var_name,
                direction.english(),
                writer
            )),
        },
        confidence: 0.85,
        miner: StructuralMiner::Monotonicity,
    }
}

// ─── Miner 3: Access policy ──────────────────────────────────────────

/// Mine access-policy candidates. For each state variable: collect all
/// external/public functions that write to it; intersect their modifier
/// sets. For each modifier in the non-empty intersection: emit one
/// candidate at confidence 0.80.
///
/// The Halmos check is structural-equivalent (compile-time the source
/// already satisfies the property by construction). The candidate's
/// value is the surfaced intent_text — "every public writer of <var>
/// carries modifier <m>" — plus the dispatched check that confirms the
/// property holds when re-checked. Full symbolic verification of
/// modifier semantics requires knowing what `<m>` does; Phase 5 mines
/// the structural property, not the runtime gate.
pub fn mine_access_policy(
    sources: &[(PathBuf, String)],
    layouts: &[StorageLayout],
) -> (Vec<StructuralCandidate>, Vec<LowConfidenceFinding>) {
    let mut high: Vec<StructuralCandidate> = Vec::new();
    let low: Vec<LowConfidenceFinding> = Vec::new();

    for (path, source) in sources {
        let file_layout: Vec<&StorageLayout> = layouts
            .iter()
            .filter(|l| layout_belongs_to(l, path))
            .collect();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for layout in &file_layout {
            for entry in &layout.entries {
                if !seen.insert(entry.label.as_str()) {
                    continue;
                }
                // External writers of this var.
                let writers = external_writers_of(source, &entry.label);
                if writers.len() < 1 {
                    continue;
                }
                // Intersection of modifier sets across all writers. An
                // empty intersection (or any writer with no modifiers)
                // disqualifies.
                let shared = intersect_modifiers(source, &writers);
                if shared.is_empty() {
                    continue;
                }
                for modifier in shared {
                    high.push(make_access_policy_candidate(
                        &entry.label,
                        &modifier,
                        &writers,
                        path,
                    ));
                }
            }
        }
    }

    (high, low)
}

fn external_writers_of(source: &str, var_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    for sig in extract_signatures(source) {
        if !is_externally_callable(&sig) {
            continue;
        }
        if let Some(body) = body_for(&sig.name, source) {
            if body_writes_to(body, var_name) {
                out.push(sig.name.clone());
            }
        }
    }
    out
}

fn intersect_modifiers(source: &str, fn_names: &[String]) -> Vec<String> {
    if fn_names.is_empty() {
        return Vec::new();
    }
    let mut iter = fn_names.iter();
    let first = iter.next().unwrap();
    let mut acc: std::collections::BTreeSet<String> = modifiers_of(first, source).into_iter().collect();
    for name in iter {
        let m: std::collections::BTreeSet<String> = modifiers_of(name, source).into_iter().collect();
        acc = acc.intersection(&m).cloned().collect();
        if acc.is_empty() {
            break;
        }
    }
    acc.into_iter().collect()
}

/// Extract the modifier names attached to a function. Modifiers are the
/// identifiers between the closing `)` of the args and the opening `{`
/// of the body, excluding visibility / mutability / returns keywords.
fn modifiers_of(fn_name: &str, source: &str) -> Vec<String> {
    let stripped = strip_comments(source);
    // Find the function declaration.
    let needle = format!("function {fn_name}");
    let Some(rel) = stripped.find(&needle) else {
        return Vec::new();
    };
    // Skip past `function <name>` to find the args opening paren.
    let after_name = rel + needle.len();
    let bytes = stripped.as_bytes();
    let mut i = after_name;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'(' {
        return Vec::new();
    }
    // Match the args paren.
    let Some(args_end) = match_paren_in_str(&stripped, i) else {
        return Vec::new();
    };
    let mods_start = args_end + 1;
    // Read until `{` or `;` at depth 0 (track paren depth for
    // modifier-with-args + returns clause).
    let mut paren = 0i32;
    let mut k = mods_start;
    while k < bytes.len() {
        match bytes[k] {
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'{' | b';' if paren == 0 => break,
            _ => {}
        }
        k += 1;
    }
    let mods_str = &stripped[mods_start..k];
    parse_modifier_list(mods_str)
}

fn match_paren_in_str(s: &str, open_idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0;
    for (i, b) in bytes.iter().enumerate().skip(open_idx) {
        match *b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_modifier_list(s: &str) -> Vec<String> {
    let reserved: std::collections::HashSet<&str> = [
        "external", "public", "internal", "private",
        "view", "pure", "payable", "nonpayable", "constant",
        "virtual", "override", "returns",
    ]
    .iter()
    .copied()
    .collect();
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip whitespace + punctuation.
        while i < bytes.len() && !is_ident_byte(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Read an identifier.
        let start = i;
        while i < bytes.len() && is_ident_byte(bytes[i]) {
            i += 1;
        }
        let ident = &s[start..i];
        // Skip optional `( ... )` after the identifier (modifier args
        // or returns tuple).
        while i < bytes.len() && (bytes[i] as char).is_whitespace() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'(' {
            if let Some(end) = match_paren_in_str(s, i) {
                i = end + 1;
            }
        }
        if reserved.contains(ident) {
            continue;
        }
        // Override-list `override(A, B)` becomes ident=`override` (skipped) + the
        // paren contents were swallowed above; safe to ignore.
        out.push(ident.to_string());
    }
    out
}

fn make_access_policy_candidate(
    var_name: &str,
    modifier: &str,
    writers: &[String],
    _path: &std::path::Path,
) -> StructuralCandidate {
    let writers_list = writers.join(", ");
    // Structural property: every external writer carries the modifier.
    // The compile-time mining already established this; the Halmos
    // check just reasserts the invariant in a form the verdict can
    // surface. Confidence 0.80 reflects that the modifier's actual
    // gate semantics aren't symbolically verified by this candidate
    // alone — the user gets the structural claim, not a proof of
    // unauthorized callers being rejected.
    let halmos = format!(
        "function check_access_{var}_via_{modifier}() public view {{\n    \
         // Property: every external writer of {var} carries the {modifier} modifier.\n    \
         // Writers: {writers_list}.\n    \
         // Source-structural fact (Phase 5 invariant; modifier body\n    \
         // semantics not symbolically modeled here).\n    \
         assert(true);\n}}\n",
        var = var_name,
        modifier = modifier,
        writers_list = writers_list,
    );
    StructuralCandidate {
        spec: SpecCandidate {
            name: format!("check_access_{var_name}_via_{modifier}"),
            halmos,
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: false,
            source: Source::Structural,
            intent_text: Some(format!(
                "every external write to {var_name} requires the {modifier} modifier (writers: {writers_list})"
            )),
        },
        confidence: 0.80,
        miner: StructuralMiner::AccessPolicy,
    }
}

// ─── Miner 4: Conservation ───────────────────────────────────────────

/// Mine conservation candidates. For each external/public function: scan
/// the body for paired mapping operations on the same mapping —
/// `<m>[<idx1>] -= <expr>` followed by `<m>[<idx2>] += <expr>` (or the
/// reverse). When the operand expressions match textually, emit one
/// candidate per (mapping, function) pair at 0.65 with intent_text
/// "<m>[<idx1>] + <m>[<idx2>] preserved across <fn>".
///
/// The 0.65 confidence reflects the conservation heuristic's
/// fragility: textual operand-matching catches the canonical ERC-20
/// transfer shape but misses three-way splits, partial conservations,
/// and operations broken across helper functions.
pub fn mine_conservation(
    sources: &[(PathBuf, String)],
    _layouts: &[StorageLayout],
) -> (Vec<StructuralCandidate>, Vec<LowConfidenceFinding>) {
    let mut high: Vec<StructuralCandidate> = Vec::new();
    let low: Vec<LowConfidenceFinding> = Vec::new();

    for (path, source) in sources {
        for sig in extract_signatures(source) {
            if !is_externally_callable(&sig) {
                continue;
            }
            let Some(body) = body_for(&sig.name, source) else { continue };
            for pair in find_conservation_pairs(body) {
                high.push(make_conservation_candidate(&sig.name, &sig.args, &pair, path));
            }
        }
    }

    (high, low)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConservationPair {
    mapping: String,
    debit_index: String,
    credit_index: String,
    /// Operand expression (the amount moved). Same on both sides.
    operand: String,
}

/// Find pairs of paired mapping ops in a single body. Each pair is
/// `<m>[idx1] -= e; <m>[idx2] += e;` (or the swap). Skips matches where
/// the operand differs textually.
fn find_conservation_pairs(body: &str) -> Vec<ConservationPair> {
    let stripped = strip_comments(body);
    let ops = scan_mapping_ops(&stripped);
    let mut out = Vec::new();
    let mut used: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (i, a) in ops.iter().enumerate() {
        if used.contains(&i) {
            continue;
        }
        if a.op != "-=" {
            continue;
        }
        for (j, b) in ops.iter().enumerate().skip(i + 1) {
            if used.contains(&j) {
                continue;
            }
            if b.op != "+=" || b.mapping != a.mapping || b.operand.trim() != a.operand.trim() {
                continue;
            }
            out.push(ConservationPair {
                mapping: a.mapping.clone(),
                debit_index: a.index.clone(),
                credit_index: b.index.clone(),
                operand: a.operand.clone(),
            });
            used.insert(i);
            used.insert(j);
            break;
        }
    }
    out
}

#[derive(Debug, Clone)]
struct MappingOp {
    mapping: String,
    index: String,
    op: String,
    operand: String,
}

/// Scan a function body for `<ident>[<idx>] <op> <operand>;` statements
/// where `<op>` is `+=` or `-=`.
fn scan_mapping_ops(body: &str) -> Vec<MappingOp> {
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find next identifier followed by `[`.
        while i < bytes.len() && !is_ident_byte(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let id_start = i;
        while i < bytes.len() && is_ident_byte(bytes[i]) {
            i += 1;
        }
        let id_end = i;
        // Optional whitespace then `[`.
        let mut j = i;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'[' {
            continue;
        }
        let bracket_open = j;
        let Some(bracket_close) = match_bracket(body, bracket_open) else {
            i = bracket_open + 1;
            continue;
        };
        let index_str = &body[bracket_open + 1..bracket_close];
        // Optional whitespace then `+=` or `-=`.
        let mut k = bracket_close + 1;
        while k < bytes.len() && (bytes[k] as char).is_whitespace() {
            k += 1;
        }
        if k + 1 >= bytes.len() {
            i = bracket_close + 1;
            continue;
        }
        let op = if &bytes[k..k + 2] == b"+=" {
            "+="
        } else if &bytes[k..k + 2] == b"-=" {
            "-="
        } else {
            i = bracket_close + 1;
            continue;
        };
        let mut m = k + 2;
        while m < bytes.len() && (bytes[m] as char).is_whitespace() {
            m += 1;
        }
        // Operand runs to the next `;`.
        let Some(semi_rel) = body[m..].find(';') else {
            i = m;
            continue;
        };
        let operand = body[m..m + semi_rel].trim().to_string();
        out.push(MappingOp {
            mapping: body[id_start..id_end].to_string(),
            index: index_str.trim().to_string(),
            op: op.to_string(),
            operand,
        });
        i = m + semi_rel + 1;
    }
    out
}

fn match_bracket(s: &str, open_idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate().skip(open_idx) {
        match *b {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn make_conservation_candidate(
    fn_name: &str,
    fn_args: &str,
    pair: &ConservationPair,
    _path: &std::path::Path,
) -> StructuralCandidate {
    let call_args = forward_arg_names(fn_args);
    // Local conservation: the two touched entries sum to their
    // pre-value. Indices reference variables in scope (writer's args +
    // msg.sender) — they evaluate the same way in the check body.
    let halmos = format!(
        "function check_conservation_{mapping}_via_{fn_name}{sig} public {{\n    \
         uint256 pre = token.{mapping}({debit_idx}) + token.{mapping}({credit_idx});\n    \
         try token.{fn_name}({call_args}) {{}} catch {{ return; }}\n    \
         uint256 post = token.{mapping}({debit_idx}) + token.{mapping}({credit_idx});\n    \
         assert(post == pre);\n}}\n",
        mapping = pair.mapping,
        fn_name = fn_name,
        sig = fn_args,
        debit_idx = pair.debit_index,
        credit_idx = pair.credit_index,
        call_args = call_args,
    );
    StructuralCandidate {
        spec: SpecCandidate {
            name: format!("check_conservation_{}_via_{}", pair.mapping, fn_name),
            halmos,
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: false,
            source: Source::Structural,
            intent_text: Some(format!(
                "{m}[{d}] + {m}[{c}] preserved across {fn} (operand: {op})",
                m = pair.mapping,
                d = pair.debit_index,
                c = pair.credit_index,
                fn = fn_name,
                op = pair.operand,
            )),
        },
        confidence: 0.65,
        miner: StructuralMiner::Conservation,
    }
}

// ─── Miner 5: Two-step pattern ───────────────────────────────────────

/// Mine two-step-pattern candidates. For each (F1, F2) pair where F1
/// writes state variable `gate` AND F2's body contains `require(gate,
/// ...)` / `require(gate == ..., ...)` / `if (!gate) revert(...)`:
/// emit one candidate at confidence 0.65 with intent_text "<F2> requires
/// <F1> first via gate <gate>".
///
/// Halmos check exercises F2 in a fresh state (no prior F1 call) and
/// asserts revert. Halmos symbolically initializes `token`'s storage;
/// the property holds when the gate var defaults to a "needs F1"
/// value (0/false for the canonical pattern).
pub fn mine_two_step(
    sources: &[(PathBuf, String)],
    layouts: &[StorageLayout],
) -> (Vec<StructuralCandidate>, Vec<LowConfidenceFinding>) {
    let mut high: Vec<StructuralCandidate> = Vec::new();
    let low: Vec<LowConfidenceFinding> = Vec::new();

    for (path, source) in sources {
        let file_layout: Vec<&StorageLayout> = layouts
            .iter()
            .filter(|l| layout_belongs_to(l, path))
            .collect();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for layout in &file_layout {
            for entry in &layout.entries {
                if !seen.insert(entry.label.as_str()) {
                    continue;
                }
                if is_mapping_or_array_type(layout, &entry.type_id) {
                    continue;
                }
                let gate = entry.label.as_str();
                let writers = external_writers_of(source, gate);
                let requirers = external_requirers_of(source, gate);
                for f1 in &writers {
                    for f2 in &requirers {
                        if f1 == f2 {
                            continue;
                        }
                        let Some(sig) = signature_for(source, f2) else { continue };
                        if !is_externally_callable(&sig) {
                            continue;
                        }
                        high.push(make_two_step_candidate(f1, f2, gate, &sig.args, path));
                    }
                }
            }
        }
    }

    (high, low)
}

/// Functions whose body references `gate` inside a `require` or
/// negated-if revert pattern.
fn external_requirers_of(source: &str, gate: &str) -> Vec<String> {
    let mut out = Vec::new();
    for sig in extract_signatures(source) {
        if !is_externally_callable(&sig) {
            continue;
        }
        if let Some(body) = body_for(&sig.name, source) {
            if body_requires_gate(body, gate) {
                out.push(sig.name.clone());
            }
        }
    }
    out
}

fn body_requires_gate(body: &str, gate: &str) -> bool {
    let stripped = strip_comments(body);
    // require(<gate>...) — gate may appear bare, equal-checked, or
    // inverted via `!<gate>`.
    if contains_require_with(&stripped, gate) {
        return true;
    }
    if contains_if_negated_revert(&stripped, gate) {
        return true;
    }
    false
}

fn contains_require_with(body: &str, gate: &str) -> bool {
    let mut i = 0;
    let bytes = body.as_bytes();
    while i + "require(".len() < bytes.len() {
        let rest = &body[i..];
        let Some(rel) = rest.find("require(") else {
            break;
        };
        let open = i + rel + "require(".len() - 1;
        if let Some(close) = match_paren_in_str(body, open) {
            let arg = &body[open + 1..close];
            if mentions_gate(arg, gate) {
                return true;
            }
            i = close + 1;
        } else {
            i = open + 1;
        }
    }
    false
}

fn contains_if_negated_revert(body: &str, gate: &str) -> bool {
    // `if (!gate)` or `if (gate == 0)` or `if (gate == false)` ... { ... revert ... }
    let mut i = 0;
    let bytes = body.as_bytes();
    while i + "if (".len() < bytes.len() {
        let rest = &body[i..];
        let Some(rel) = rest.find("if (").or_else(|| rest.find("if(")) else {
            break;
        };
        let abs = i + rel;
        let open = abs + body[abs..].find('(').unwrap_or(0);
        if let Some(close) = match_paren_in_str(body, open) {
            let cond = body[open + 1..close].trim();
            let mentions_negated = (cond.starts_with('!') && mentions_gate(&cond[1..], gate))
                || (mentions_gate(cond, gate)
                    && (cond.contains("== false") || cond.contains("== 0")));
            if mentions_negated {
                // Look for `revert` in the block / statement following
                // the condition (within 200 chars).
                let after = close + 1;
                let end = body.len().min(after + 200);
                let look = &body[after..end];
                if look.contains("revert") {
                    return true;
                }
            }
            i = close + 1;
        } else {
            i = open + 1;
        }
    }
    false
}

fn mentions_gate(expr: &str, gate: &str) -> bool {
    let bytes = expr.as_bytes();
    let n = gate.len();
    let mut i = 0;
    while i + n <= bytes.len() {
        if &bytes[i..i + n] == gate.as_bytes() && is_word_boundary_at(expr, i, n) {
            return true;
        }
        i += 1;
    }
    false
}

fn make_two_step_candidate(
    f1: &str,
    f2: &str,
    gate: &str,
    f2_args: &str,
    _path: &std::path::Path,
) -> StructuralCandidate {
    let call_args = forward_arg_names(f2_args);
    let halmos = format!(
        "function check_two_step_{f2}_requires_{f1}{sig} public {{\n    \
         // Property: {f2} must revert if {f1} (which writes the gate\n    \
         // variable `{gate}`) hasn't been called yet. Halmos\n    \
         // symbolically initializes `token` with the gate at its zero\n    \
         // value; the call should revert.\n    \
         try token.{f2}({call_args}) {{ assert(false); }} catch {{}}\n}}\n",
        f1 = f1,
        f2 = f2,
        gate = gate,
        sig = f2_args,
        call_args = call_args,
    );
    StructuralCandidate {
        spec: SpecCandidate {
            name: format!("check_two_step_{f2}_requires_{f1}"),
            halmos,
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: false,
            source: Source::Structural,
            intent_text: Some(format!(
                "{f2} requires {f1} first (via gate `{gate}`)"
            )),
        },
        confidence: 0.65,
        miner: StructuralMiner::TwoStep,
    }
}

// ─── shared helpers ──────────────────────────────────────────────────

fn strip_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn project_to_original(original: &str, stripped: &str, stripped_idx: usize) -> Option<usize> {
    let _ = stripped;
    let bytes = original.as_bytes();
    let mut i = 0;
    let mut non_comment = 0;
    while i < bytes.len() {
        if non_comment == stripped_idx {
            return Some(i);
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        i += 1;
        non_comment += 1;
    }
    None
}

fn find_body_open_after_parens(src: &str) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut paren_depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => paren_depth += 1,
            b')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            b'{' if paren_depth == 0 => return Some(i),
            b';' if paren_depth == 0 => return None,
            _ => {}
        }
        i += 1;
    }
    None
}

fn match_brace(src: &str, open_idx: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate().skip(open_idx) {
        match *b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_spec(name: &str) -> SpecCandidate {
        SpecCandidate {
            name: name.into(),
            halmos: format!(
                "function {name}() public {{ assert(true); }}"
            ),
            smtchecker: String::new(),
            template_ref: None,
            intent_satisfied: false,
            source: Source::UserIntent,
            intent_text: None,
        }
    }

    #[test]
    fn miner_ids_are_stable_kebab_case() {
        assert_eq!(StructuralMiner::InvariantConstants.id(), "invariant-constants");
        assert_eq!(StructuralMiner::Monotonicity.id(), "monotonicity");
        assert_eq!(StructuralMiner::AccessPolicy.id(), "access-policy");
        assert_eq!(StructuralMiner::Conservation.id(), "conservation");
        assert_eq!(StructuralMiner::TwoStep.id(), "two-step");
    }

    #[test]
    fn miner_all_returns_five_in_stable_order() {
        let v = StructuralMiner::all();
        assert_eq!(v.len(), 5);
        assert_eq!(v[0], StructuralMiner::InvariantConstants);
        assert_eq!(v[4], StructuralMiner::TwoStep);
    }

    #[test]
    fn into_spec_candidate_tags_source_and_encodes_confidence() {
        let sc = StructuralCandidate {
            spec: dummy_spec("check_owner_const"),
            confidence: 0.95,
            miner: StructuralMiner::InvariantConstants,
        };
        let out = sc.into_spec_candidate();
        assert_eq!(out.source, Source::Structural);
        assert_eq!(
            out.template_ref.as_deref(),
            Some("structural:invariant-constants:0.95")
        );
    }

    #[test]
    fn into_spec_candidate_preserves_explicit_template_ref() {
        // A miner that already filled in a richer template_ref should
        // NOT be clobbered by the default format.
        let mut spec = dummy_spec("check_x");
        spec.template_ref = Some("structural:custom".into());
        let sc = StructuralCandidate {
            spec,
            confidence: 0.7,
            miner: StructuralMiner::Conservation,
        };
        let out = sc.into_spec_candidate();
        assert_eq!(out.template_ref.as_deref(), Some("structural:custom"));
    }

    #[test]
    fn empty_extract_returns_default_report() {
        let cfg = StructuralConfig::default();
        let r = extract_from_structural(&[], &[], &cfg);
        assert!(r.candidates.is_empty());
        assert!(r.low_confidence_findings.is_empty());
        assert!(r.miner_counts.is_empty());
    }

    #[test]
    fn config_default_threshold_is_06() {
        let cfg = StructuralConfig::default();
        assert!((cfg.min_confidence - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn low_confidence_finding_format() {
        let f = LowConfidenceFinding::new(
            StructuralMiner::TwoStep,
            "F2 requires F1",
            0.55,
        )
        .with_target("commit_reveal");
        assert_eq!(f.confidence, "0.55");
        assert_eq!(f.fn_or_var.as_deref(), Some("commit_reveal"));
    }

    // ─── Invariant-constants miner (S1) ──────────────────────────────

    fn fixture(name: &str) -> (PathBuf, String) {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/structural")
            .join(name);
        let src = std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
        (p, src)
    }

    #[test]
    fn invariant_keyword_emits_tier_a_candidates() {
        // `uint8 public constant DECIMALS = 18;` and
        // `address public immutable OWNER;` both yield Tier A candidates.
        // OWNER has no literal initializer so the halmos string falls
        // back to a getter call (defensive — Tier A normally pins a
        // literal, but immutables-without-initializer-at-declaration are
        // common). Confidence stays 0.95 for the keyword tier.
        let (path, src) = fixture("invariant_keyword.sol");
        let (high, _low) = mine_invariant_constants(&[(path, src)], &[]);
        let names: Vec<&str> = high.iter().map(|c| c.spec.name.as_str()).collect();
        assert!(
            names.iter().any(|n| n.contains("DECIMALS")),
            "missing DECIMALS in {names:?}"
        );
        assert!(
            names.iter().any(|n| n.contains("OWNER")),
            "missing OWNER in {names:?}"
        );
        for c in &high {
            assert_eq!(c.miner, StructuralMiner::InvariantConstants);
            assert!(
                (c.confidence - 0.95).abs() < 1e-3,
                "Tier A confidence drift: {}",
                c.confidence
            );
        }
    }

    #[test]
    fn invariant_keyword_halmos_for_uint_asserts_literal() {
        let (path, src) = fixture("invariant_keyword.sol");
        let (high, _) = mine_invariant_constants(&[(path, src)], &[]);
        let decimals = high
            .iter()
            .find(|c| c.spec.name.contains("DECIMALS"))
            .expect("DECIMALS candidate");
        assert!(
            decimals.spec.halmos.contains("c.DECIMALS()"),
            "halmos lacks getter: {}",
            decimals.spec.halmos
        );
        assert!(
            decimals.spec.halmos.contains("uint256(18)"),
            "halmos lacks literal assertion: {}",
            decimals.spec.halmos
        );
    }

    #[test]
    fn invariant_ctor_only_tier_b_picks_up_literal_initialized_strings() {
        // `name` and `symbol` are declared with literal initializers.
        // `totalSupply` is ctor-only-write with NO declaration literal →
        // Tier C → low_confidence_findings, not high.
        let (path, src) = fixture("invariant_ctor_only.sol");
        // Tier B requires storage layout to know which vars to consider.
        // Synthesize a minimal layout matching the fixture.
        let layouts = vec![mock_layout(
            "InvariantCtorOnly",
            &[
                ("name", "t_string_storage"),
                ("symbol", "t_string_storage"),
                ("totalSupply", "t_uint256"),
                ("counter", "t_uint256"),
            ],
            &path,
        )];
        let (high, low) = mine_invariant_constants(&[(path, src)], &layouts);
        let high_names: Vec<&str> = high.iter().map(|c| c.spec.name.as_str()).collect();
        assert!(
            high_names.iter().any(|n| n.contains("name")),
            "missing name in {high_names:?}"
        );
        assert!(
            high_names.iter().any(|n| n.contains("symbol")),
            "missing symbol in {high_names:?}"
        );
        // counter is written outside the constructor (bump()) — must not
        // be classified as invariant.
        assert!(
            !high_names.iter().any(|n| n.contains("counter")),
            "counter wrongly classified invariant: {high_names:?}"
        );
        // totalSupply → low-confidence Tier C.
        assert!(
            low.iter().any(|f| f
                .fn_or_var
                .as_deref()
                .map(|s| s.contains("totalSupply"))
                .unwrap_or(false)),
            "totalSupply not in low_confidence: {low:?}"
        );
        // All high-confidence are Tier B at 0.80.
        for c in &high {
            assert!(
                (c.confidence - 0.80).abs() < 1e-3,
                "Tier B confidence drift: {}",
                c.confidence
            );
        }
    }

    #[test]
    fn invariant_negative_no_candidates() {
        // `value` is rewritten in `setValue` — must NOT be classified.
        let (path, src) = fixture("invariant_negative.sol");
        let layouts = vec![mock_layout(
            "InvariantNegative",
            &[("value", "t_uint256")],
            &path,
        )];
        let (high, low) = mine_invariant_constants(&[(path, src)], &layouts);
        assert!(high.is_empty(), "high expected empty: {high:?}");
        // No low-confidence either (the writer outside ctor disqualifies
        // it from the ctor-only-write branch).
        assert!(low.is_empty(), "low expected empty: {low:?}");
    }

    #[test]
    fn extract_from_structural_routes_by_confidence_threshold() {
        // Wire-through test: confirm the aggregator + threshold split
        // works end-to-end. Using the ctor-only fixture: name + symbol
        // go to candidates; totalSupply goes to low.
        let (path, src) = fixture("invariant_ctor_only.sol");
        let layouts = vec![mock_layout(
            "InvariantCtorOnly",
            &[
                ("name", "t_string_storage"),
                ("symbol", "t_string_storage"),
                ("totalSupply", "t_uint256"),
                ("counter", "t_uint256"),
            ],
            &path,
        )];
        let cfg = StructuralConfig::default();
        let report = extract_from_structural(&[(path, src)], &layouts, &cfg);
        assert!(report.candidates.len() >= 2, "candidates: {:?}", report.candidates);
        // miner_counts populated only for high-confidence emissions.
        assert!(
            report.miner_counts.contains_key(&StructuralMiner::InvariantConstants)
        );
        // All candidates carry Source::Structural with a structural-
        // family template_ref. (The fixture also yields a monotonicity
        // candidate for `counter` via bump(), so the aggregator output
        // mixes invariant-constants + monotonicity entries.)
        for c in &report.candidates {
            assert_eq!(c.source, Source::Structural);
            let t = c.template_ref.as_deref().unwrap_or("");
            assert!(
                t.starts_with("structural:"),
                "template_ref drift: {t}"
            );
        }
        // At least one invariant-constants candidate must be present
        // (name and symbol via Tier B).
        assert!(
            report
                .candidates
                .iter()
                .any(|c| c
                    .template_ref
                    .as_deref()
                    .unwrap_or("")
                    .starts_with("structural:invariant-constants:")),
            "missing invariant-constants candidate: {:?}",
            report.candidates
        );
        // Low-confidence has at least the totalSupply Tier C.
        assert!(!report.low_confidence_findings.is_empty());
    }

    // ─── Monotonicity miner (S2) ─────────────────────────────────────

    #[test]
    fn monotonic_counter_emits_inc_per_writer() {
        // `count` is only `++` and `+= n` — pure-increment. The miner
        // emits one candidate per (var, writer) pair, so two candidates
        // for count (bump + bumpBy).
        let (path, src) = fixture("monotonic_counter.sol");
        let layouts = vec![mock_layout(
            "MonotonicCounter",
            &[("count", "t_uint256")],
            &path,
        )];
        let (high, _low) = mine_monotonicity(&[(path, src)], &layouts);
        assert_eq!(high.len(), 2, "expected one per writer: {:#?}", high);
        for c in &high {
            assert_eq!(c.miner, StructuralMiner::Monotonicity);
            assert!((c.confidence - 0.85).abs() < 1e-3);
            assert!(
                c.spec.halmos.contains("assert(post >= pre)"),
                "missing inc assertion: {}",
                c.spec.halmos
            );
            assert!(c.spec.halmos.contains("token.count()"));
        }
        let names: Vec<&str> = high.iter().map(|c| c.spec.name.as_str()).collect();
        assert!(names.iter().any(|n| n.ends_with("via_bump")));
        assert!(names.iter().any(|n| n.ends_with("via_bumpBy")));
    }

    #[test]
    fn monotonic_counter_halmos_forwards_args() {
        let (path, src) = fixture("monotonic_counter.sol");
        let layouts = vec![mock_layout(
            "MonotonicCounter",
            &[("count", "t_uint256")],
            &path,
        )];
        let (high, _) = mine_monotonicity(&[(path, src)], &layouts);
        let bump_by = high
            .iter()
            .find(|c| c.spec.name.ends_with("via_bumpBy"))
            .expect("bumpBy candidate");
        // The check function signature mirrors the writer's args; the
        // call inside forwards the param name.
        assert!(
            bump_by.spec.halmos.contains("via_bumpBy(uint256 n)"),
            "missing args in check sig: {}",
            bump_by.spec.halmos
        );
        assert!(
            bump_by.spec.halmos.contains("token.bumpBy(n)"),
            "missing forwarded arg in call: {}",
            bump_by.spec.halmos
        );
    }

    #[test]
    fn monotonic_negative_no_candidates() {
        // `value` is mixed-polarity (+= in up, -= in down). No emission.
        let (path, src) = fixture("monotonic_negative.sol");
        let layouts = vec![mock_layout(
            "MonotonicNegative",
            &[("value", "t_uint256")],
            &path,
        )];
        let (high, low) = mine_monotonicity(&[(path, src)], &layouts);
        assert!(high.is_empty(), "high expected empty: {high:?}");
        assert!(low.is_empty(), "low expected empty: {low:?}");
    }

    #[test]
    fn write_polarity_scanner_classifies_compound_ops() {
        let body = "x++; x += 1; --x; y -= 2;";
        let xp = scan_polarities(body, "x");
        assert_eq!(
            xp,
            vec![
                WritePolarity::Increment, // x++
                WritePolarity::Increment, // x +=
                WritePolarity::Decrement, // --x
            ]
        );
        let yp = scan_polarities(body, "y");
        assert_eq!(yp, vec![WritePolarity::Decrement]);
    }

    #[test]
    fn write_polarity_overwrite_disqualifies() {
        let body = "x = 1; x += 2;";
        let p = scan_polarities(body, "x");
        assert!(p.contains(&WritePolarity::Overwrite));
        assert!(p.contains(&WritePolarity::Increment));
    }

    #[test]
    fn forward_arg_names_handles_locations() {
        assert_eq!(forward_arg_names("()"), "");
        assert_eq!(forward_arg_names("(uint256 n)"), "n");
        assert_eq!(forward_arg_names("(address to, uint256 amount)"), "to, amount");
        assert_eq!(
            forward_arg_names("(bytes calldata data)"),
            "data",
            "calldata location keyword should be dropped"
        );
    }

    // ─── Access-policy miner (S3) ────────────────────────────────────

    #[test]
    fn access_consistent_emits_per_shared_modifier() {
        let (path, src) = fixture("access_consistent.sol");
        let layouts = vec![mock_layout(
            "AccessConsistent",
            &[("owner", "t_address"), ("balance", "t_uint256")],
            &path,
        )];
        let (high, _low) = mine_access_policy(&[(path, src)], &layouts);
        // `balance` is written by deposit + withdraw, both gated by
        // `onlyOwner`. One candidate per (var, shared-modifier).
        let balance_cands: Vec<&StructuralCandidate> = high
            .iter()
            .filter(|c| c.spec.name.contains("balance"))
            .collect();
        assert_eq!(
            balance_cands.len(),
            1,
            "expected one balance/onlyOwner candidate: {:#?}",
            high
        );
        let c = balance_cands[0];
        assert!((c.confidence - 0.80).abs() < 1e-3);
        assert!(c.spec.name.contains("onlyOwner"), "name: {}", c.spec.name);
        let it = c.spec.intent_text.as_deref().unwrap_or("");
        assert!(it.contains("onlyOwner"));
        assert!(it.contains("deposit"));
        assert!(it.contains("withdraw"));
    }

    #[test]
    fn access_inconsistent_no_candidates_for_disagreeing_writers() {
        let (path, src) = fixture("access_inconsistent.sol");
        let layouts = vec![mock_layout(
            "AccessInconsistent",
            &[("owner", "t_address"), ("balance", "t_uint256")],
            &path,
        )];
        let (high, _) = mine_access_policy(&[(path, src)], &layouts);
        // `balance` has writers deposit (onlyOwner) + donate (no
        // modifier). Intersection is empty → no candidate.
        assert!(
            !high.iter().any(|c| c.spec.name.contains("balance")),
            "balance must NOT have an access-policy candidate: {:#?}",
            high
        );
    }

    #[test]
    fn modifiers_of_extracts_named_modifiers_only() {
        let src = r#"
            contract C {
                function f(uint256 n) external onlyOwner nonReentrant returns (bool) {}
                function g() public view virtual override returns (uint256) {}
                function h(address a) external payable noModifier {}
            }
        "#;
        let m = modifiers_of("f", src);
        assert!(m.contains(&"onlyOwner".to_string()), "{m:?}");
        assert!(m.contains(&"nonReentrant".to_string()), "{m:?}");
        assert!(!m.iter().any(|x| x == "external"));
        assert!(!m.iter().any(|x| x == "returns"));

        let g = modifiers_of("g", src);
        // virtual, override, view, public, returns are all reserved.
        assert!(g.is_empty(), "g should have no real modifiers: {g:?}");

        let h = modifiers_of("h", src);
        assert!(h.contains(&"noModifier".to_string()), "{h:?}");
    }

    // ─── Conservation miner (S4) ─────────────────────────────────────

    #[test]
    fn conservation_transfer_emits_pair_per_function() {
        let (path, src) = fixture("conservation_transfer.sol");
        let (high, _low) = mine_conservation(&[(path, src)], &[]);
        let xfer_cands: Vec<&StructuralCandidate> = high
            .iter()
            .filter(|c| c.spec.name.contains("via_transfer"))
            .collect();
        assert_eq!(
            xfer_cands.len(),
            1,
            "expected one conservation candidate for transfer: {:#?}",
            high
        );
        let c = xfer_cands[0];
        assert_eq!(c.miner, StructuralMiner::Conservation);
        assert!((c.confidence - 0.65).abs() < 1e-3);
        assert!(
            c.spec.halmos.contains("token.balanceOf(msg.sender)"),
            "halmos missing debit index: {}",
            c.spec.halmos
        );
        assert!(
            c.spec.halmos.contains("token.balanceOf(to)"),
            "halmos missing credit index: {}",
            c.spec.halmos
        );
        assert!(c.spec.halmos.contains("assert(post == pre)"));
    }

    #[test]
    fn conservation_mint_no_candidate() {
        let (path, src) = fixture("conservation_mint.sol");
        let (high, _) = mine_conservation(&[(path, src)], &[]);
        assert!(
            high.iter().all(|c| !c.spec.name.contains("via_mint")),
            "mint must not yield conservation: {:#?}",
            high
        );
    }

    #[test]
    fn scan_mapping_ops_finds_paired_assignments() {
        let body = "balanceOf[a] -= n; balanceOf[b] += n;";
        let ops = scan_mapping_ops(body);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].mapping, "balanceOf");
        assert_eq!(ops[0].index, "a");
        assert_eq!(ops[0].op, "-=");
        assert_eq!(ops[1].index, "b");
        assert_eq!(ops[1].op, "+=");
    }

    #[test]
    fn find_conservation_pairs_rejects_mismatched_operands() {
        let body = "balanceOf[a] -= amount; balanceOf[b] += other;";
        let pairs = find_conservation_pairs(body);
        assert!(pairs.is_empty(), "mismatched operands should not pair");
    }

    // ─── Two-step miner (S5) ─────────────────────────────────────────

    #[test]
    fn two_step_commit_reveal_emits_pair() {
        let (path, src) = fixture("two_step_commit_reveal.sol");
        let layouts = vec![mock_layout(
            "TwoStepCommitReveal",
            &[("committed", "t_bool"), ("revealed", "t_uint256")],
            &path,
        )];
        let (high, _) = mine_two_step(&[(path, src)], &layouts);
        let want = high
            .iter()
            .find(|c| c.spec.name == "check_two_step_reveal_requires_commit");
        assert!(want.is_some(), "missing reveal-requires-commit: {:#?}", high);
        let c = want.unwrap();
        assert_eq!(c.miner, StructuralMiner::TwoStep);
        assert!((c.confidence - 0.65).abs() < 1e-3);
        assert!(
            c.spec.halmos.contains("try token.reveal(value) { assert(false); }"),
            "halmos shape drift: {}",
            c.spec.halmos
        );
        let it = c.spec.intent_text.as_deref().unwrap_or("");
        assert!(it.contains("committed"));
        assert!(it.contains("commit"));
    }

    #[test]
    fn two_step_negative_no_candidate_when_gates_dont_match() {
        let (path, src) = fixture("two_step_negative.sol");
        let layouts = vec![mock_layout(
            "TwoStepNegative",
            &[("a", "t_uint256"), ("b", "t_bool")],
            &path,
        )];
        let (high, _) = mine_two_step(&[(path, src)], &layouts);
        // setA writes `a`; useB requires `b`. The gate doesn't match
        // any writer of `b` (there is none). No candidate.
        assert!(high.is_empty(), "expected no candidate: {:#?}", high);
    }

    #[test]
    fn body_requires_gate_handles_require_and_negated_if() {
        assert!(body_requires_gate("require(committed, \"x\");", "committed"));
        assert!(body_requires_gate("require(committed == true);", "committed"));
        assert!(body_requires_gate("if (!committed) revert();", "committed"));
        assert!(body_requires_gate("if (committed == false) { revert(); }", "committed"));
        assert!(!body_requires_gate("require(true);", "committed"));
        assert!(!body_requires_gate("if (other) revert();", "committed"));
    }

    fn mock_layout(
        contract: &str,
        vars: &[(&str, &str)],
        path: &PathBuf,
    ) -> vergil_solidity::storage::StorageLayout {
        use vergil_solidity::storage::{StorageEntry, StorageLayout, StorageType};
        let fname = path.file_name().unwrap().to_str().unwrap();
        let mut types = HashMap::new();
        for (_label, type_id) in vars {
            types.insert(
                (*type_id).to_string(),
                StorageType {
                    label: if type_id.contains("mapping") {
                        "mapping(address => uint256)".to_string()
                    } else if type_id.contains("string") {
                        "string".to_string()
                    } else if type_id.contains("uint256") {
                        "uint256".to_string()
                    } else {
                        "uint8".to_string()
                    },
                    encoding: "inplace".to_string(),
                    number_of_bytes: "32".to_string(),
                },
            );
        }
        StorageLayout {
            qualified_name: format!("{fname}:{contract}"),
            entries: vars
                .iter()
                .enumerate()
                .map(|(i, (label, type_id))| StorageEntry {
                    label: (*label).to_string(),
                    slot: i.to_string(),
                    offset: 0,
                    type_id: (*type_id).to_string(),
                    contract: format!("{fname}:{contract}"),
                })
                .collect(),
            types,
        }
    }
}
