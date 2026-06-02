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
        // All candidates carry Source::Structural + invariant-constants template_ref.
        for c in &report.candidates {
            assert_eq!(c.source, Source::Structural);
            let t = c.template_ref.as_deref().unwrap_or("");
            assert!(
                t.starts_with("structural:invariant-constants:"),
                "template_ref drift: {t}"
            );
        }
        // Low-confidence has at least the totalSupply Tier C.
        assert!(!report.low_confidence_findings.is_empty());
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
