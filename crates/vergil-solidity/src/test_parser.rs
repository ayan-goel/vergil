//! Parse Foundry / Halmos test sources into structured records the LLM
//! test-derived intent extraction (Phase 4 §3.4a) can consume.
//!
//! Intentionally regex-y, not a full Solidity parser. The downstream LLM
//! only needs the test's name, doc comment, body, and a coarse list of
//! assertion sites — full AST parsing would be overkill and pull in
//! solang/solc as a dependency. Same design philosophy as
//! [`super::signatures`].
//!
//! Both Foundry (`test*` + `assertEq` / `vm.expectRevert`) and Halmos
//! (`check_*` + bare `assert(...)`) shapes are recognized. The reference
//! examples under `examples/` use Halmos check_ functions; production
//! Foundry projects use forge-std style assertions — Phase 4 needs both.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Where to look for test files inside a project directory. Foundry's
/// canonical layout puts them under `test/`; some legacy projects use
/// `tests/`. Hardhat projects keep tests under `test/` too but in
/// `.js` / `.ts` — those are deferred (see [`TestParserError::HardhatDetected`]).
const FOUNDRY_TEST_DIRS: &[&str] = &["test", "tests"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTest {
    /// Function name, e.g. `testTransferEmitsEvent` or `check_balance_conservation`.
    pub name: String,
    /// Leading doc-comment block (`///` lines or `/** ... */`), with the
    /// comment markers stripped and lines joined by `\n`. `None` when the
    /// function has no preceding doc comment.
    pub doc_comment: Option<String>,
    /// Raw function body between the outermost `{ ... }`. Newlines preserved.
    pub body: String,
    /// Assertion sites detected inside [`Self::body`]. Order matches source order.
    pub assertions: Vec<Assertion>,
    /// Path of the file the test came from, relative to whatever the caller
    /// passed to [`parse_tests`]. Useful for traceback in error messages.
    pub source_path: PathBuf,
}

/// A single assertion site. The variants are deliberately coarse — the
/// downstream LLM does not need the exact AST, only the shape so it knows
/// what kind of property to generalize from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assertion {
    /// `assertEq(lhs, rhs)` — both sides captured as raw expression text.
    Eq { lhs: String, rhs: String },
    /// `assertTrue(expr)` — Foundry / forge-std.
    True { expr: String },
    /// `assertFalse(expr)`.
    False { expr: String },
    /// `vm.expectRevert(...)` — call existence; the matched arg, if any,
    /// is dropped (the LLM gets enough signal from the presence alone).
    ExpectRevert,
    /// Bare `assert(expr)` — the Halmos style our reference examples use.
    HalmosAssert { expr: String },
}

#[derive(Debug, Error)]
pub enum TestParserError {
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Parse every Foundry test file under `project_root`. Walks
/// `test/` and `tests/` directories one level deep (Foundry's standard
/// layout) plus the project root itself if it directly contains `.t.sol`
/// files. Returns the ParsedTests in deterministic order (directory walk
/// is sorted by filename).
///
/// Hardhat projects are detected by the presence of a `package.json`
/// listing `hardhat` in dependencies; we log a warning and return the
/// Foundry-side findings (which may be empty). Phase 4 ships Foundry-only
/// per SPEC §13 open question 5 — Hardhat support is deferred to V2.
pub fn parse_tests(project_root: &Path) -> Result<Vec<ParsedTest>, TestParserError> {
    if !project_root.is_dir() {
        return Err(TestParserError::NotADirectory(project_root.to_path_buf()));
    }
    if detect_hardhat(project_root) {
        tracing::warn!(
            "Hardhat layout detected at {}; V1.5 ships Foundry-only test extraction \
             (SPEC §13 open question 5). Hardhat / JS tests are skipped.",
            project_root.display()
        );
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for sub in FOUNDRY_TEST_DIRS {
        let dir = project_root.join(sub);
        if dir.is_dir() {
            collect_sol_files(&dir, &mut files)?;
        }
    }
    // Also accept `.t.sol` directly in the project root — some single-file
    // demo layouts (and our own erc20-broken counterexample tree) put them
    // alongside the contract.
    collect_sol_files_shallow(project_root, &mut files)?;
    files.sort();
    files.dedup();

    let mut out = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).map_err(|e| TestParserError::Io {
            path: path.clone(),
            source: e,
        })?;
        let rel = path
            .strip_prefix(project_root)
            .unwrap_or(path)
            .to_path_buf();
        for mut t in parse_tests_from_source(&src) {
            t.source_path = rel.clone();
            out.push(t);
        }
    }
    Ok(out)
}

/// Parse every test function from a single Solidity source. Exposed
/// separately so the unit tests (and future LLM-extraction integration
/// tests) can feed synthetic source without touching the filesystem.
pub fn parse_tests_from_source(source: &str) -> Vec<ParsedTest> {
    // Run the structural search on a comment-blanked copy so commented-out
    // `function ...` declarations are not picked up. Byte offsets line up
    // because comments are replaced with spaces of equal length. Doc-comment
    // extraction still needs the original source — pass both through.
    let blanked = strip_comments(source);
    let mut out = Vec::new();
    let bytes = blanked.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &blanked[i..];
        let Some(rel) = rest.find("function ") else {
            break;
        };
        let fn_start = i + rel;
        let after_kw = fn_start + "function ".len();
        let mut name_end = after_kw;
        while name_end < bytes.len() && is_ident(bytes[name_end] as char) {
            name_end += 1;
        }
        let name = &blanked[after_kw..name_end];
        if !is_test_like_name(name) {
            i = name_end;
            continue;
        }
        // Match arg list parens.
        let mut j = name_end;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'(' {
            i = j.max(fn_start + 1);
            continue;
        }
        let Some(args_end) = match_paren(source, j) else {
            i = j + 1;
            continue;
        };
        // Find the opening `{` of the body (skip modifier list).
        let mut k = args_end + 1;
        let mut depth = 0;
        let mut body_open = None;
        while k < bytes.len() {
            match bytes[k] {
                b'(' => depth += 1,
                b')' if depth > 0 => depth -= 1,
                b'{' if depth == 0 => {
                    body_open = Some(k);
                    break;
                }
                b';' if depth == 0 => break, // pure declaration; no body
                _ => {}
            }
            k += 1;
        }
        let Some(body_open) = body_open else {
            i = k.max(fn_start + 1);
            continue;
        };
        let Some(body_close) = match_brace(source, body_open) else {
            i = body_open + 1;
            continue;
        };
        let body = source[body_open + 1..body_close].to_string();
        let doc_comment = extract_leading_doc_comment(source, fn_start);
        let assertions = scan_assertions(&body);
        out.push(ParsedTest {
            name: name.to_string(),
            doc_comment,
            body,
            assertions,
            source_path: PathBuf::new(),
        });
        i = body_close + 1;
    }
    out
}

/// True iff `name` matches a recognized test-function naming convention:
/// Foundry's `test*` / `testFuzz*` / `invariant*` or Halmos's `check_*`.
/// Phase 4 surfaces both into the extraction pipeline; the LLM prompt
/// distinguishes between them.
fn is_test_like_name(name: &str) -> bool {
    name.starts_with("test")
        || name.starts_with("invariant_")
        || name.starts_with("invariant")
        || name.starts_with("check_")
}

fn detect_hardhat(root: &Path) -> bool {
    let pkg = root.join("package.json");
    if let Ok(contents) = std::fs::read_to_string(&pkg) {
        contents.contains("\"hardhat\"")
    } else {
        false
    }
}

fn collect_sol_files(dir: &Path, into: &mut Vec<PathBuf>) -> Result<(), TestParserError> {
    let entries = std::fs::read_dir(dir).map_err(|e| TestParserError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| TestParserError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        if path.is_dir() {
            // Foundry test layouts are typically flat, but be tolerant of one
            // level of nesting (e.g. `test/unit/Foo.t.sol`).
            collect_sol_files(&path, into)?;
            continue;
        }
        if is_solidity_test_file(&path) {
            into.push(path);
        }
    }
    Ok(())
}

fn collect_sol_files_shallow(dir: &Path, into: &mut Vec<PathBuf>) -> Result<(), TestParserError> {
    let entries = std::fs::read_dir(dir).map_err(|e| TestParserError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| TestParserError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        if path.is_file() && is_solidity_test_file(&path) {
            into.push(path);
        }
    }
    Ok(())
}

fn is_solidity_test_file(p: &Path) -> bool {
    let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    if name.ends_with(".t.sol") {
        return true;
    }
    // Also accept Halmos-style `*Properties.sol` / `Properties.t.sol`.
    name.ends_with(".sol") && name.contains("Properties")
}

/// Scan a function body for assertion sites in source order. Detection
/// works left-to-right: at each position we check if an assertion keyword
/// begins HERE (not anywhere in the rest of the body), so the relative
/// ordering of `assert` vs `assertEq` vs `vm.expectRevert` is preserved.
fn scan_assertions(body: &str) -> Vec<Assertion> {
    let stripped = strip_string_literals(&strip_comments(body));
    let bytes = stripped.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !is_ident_start_boundary(&stripped, i) {
            i += 1;
            continue;
        }
        // Tried in longest-first order so e.g. `assertEq` doesn't get
        // shadowed by a bare `assert` match.
        if try_match_call(&stripped, i, "assertEq")
            .and_then(|(args_text, end)| {
                split_top_level_two(&args_text).map(|(lhs, rhs)| (lhs, rhs, end))
            })
            .map(|(lhs, rhs, end)| {
                out.push(Assertion::Eq {
                    lhs: lhs.trim().to_string(),
                    rhs: rhs.trim().to_string(),
                });
                i = end + 1;
            })
            .is_some()
        {
            continue;
        }
        if let Some((args, end)) = try_match_call(&stripped, i, "assertTrue") {
            out.push(Assertion::True {
                expr: args.trim().to_string(),
            });
            i = end + 1;
            continue;
        }
        if let Some((args, end)) = try_match_call(&stripped, i, "assertFalse") {
            out.push(Assertion::False {
                expr: args.trim().to_string(),
            });
            i = end + 1;
            continue;
        }
        if let Some((_, end)) = try_match_call(&stripped, i, "vm.expectRevert") {
            out.push(Assertion::ExpectRevert);
            i = end + 1;
            continue;
        }
        // Bare assert — must not match the assert* family above. The
        // longest-first dispatch handled those, so a remaining `assert`
        // here is genuinely bare.
        if try_match_keyword(&stripped, i, "assert") {
            if let Some((args, end)) = capture_call_args(&stripped, i + "assert".len()) {
                out.push(Assertion::HalmosAssert {
                    expr: args.trim().to_string(),
                });
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// True iff `at` is at the start of an identifier (i.e. preceded by a
/// non-identifier byte or the start of the string). Used to skip bytes
/// inside identifiers so `myassertEq` doesn't match.
fn is_ident_start_boundary(s: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let prev = s.as_bytes()[at - 1] as char;
    !is_ident(prev)
}

/// True iff `s[at..]` starts with `keyword` and the byte right after is
/// not an identifier byte (so `assertEq` does not match `assert`).
fn try_match_keyword(s: &str, at: usize, keyword: &str) -> bool {
    if !s.as_bytes()[at..].starts_with(keyword.as_bytes()) {
        return false;
    }
    let after = at + keyword.len();
    s.as_bytes()
        .get(after)
        .map(|b| !is_ident(*b as char))
        .unwrap_or(true)
}

/// True iff `s[at..]` begins `keyword(...)` (optionally with whitespace
/// between the name and the `(`), and if so returns the call's args text
/// and the absolute index of the matching `)`.
fn try_match_call(s: &str, at: usize, keyword: &str) -> Option<(String, usize)> {
    if !try_match_keyword(s, at, keyword) {
        return None;
    }
    capture_call_args(s, at + keyword.len())
}

/// Given that position `start` is at the start of an argument list
/// (possibly preceded by whitespace), return `(args_text, close_idx)`
/// where `args_text` is the contents between `(` and the matching `)`,
/// and `close_idx` is the absolute byte position of `)`.
fn capture_call_args(hay: &str, start: usize) -> Option<(String, usize)> {
    let bytes = hay.as_bytes();
    let mut k = start;
    while k < bytes.len() && (bytes[k] as char).is_whitespace() {
        k += 1;
    }
    if k >= bytes.len() || bytes[k] != b'(' {
        return None;
    }
    let close = match_paren(hay, k)?;
    Some((hay[k + 1..close].to_string(), close))
}

/// Split a top-level comma-delimited list of length 2. Returns `None`
/// when there are not exactly two top-level pieces. Nested parens /
/// brackets / braces are respected.
fn split_top_level_two(s: &str) -> Option<(String, String)> {
    let mut depth = 0;
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b',' if depth == 0 => {
                let lhs = &s[..i];
                let rhs = &s[i + 1..];
                // Reject a 3-arg call by checking that rhs has no top-level comma.
                if has_top_level_comma(rhs) {
                    return None;
                }
                return Some((lhs.to_string(), rhs.to_string()));
            }
            _ => {}
        }
    }
    None
}

fn has_top_level_comma(s: &str) -> bool {
    let mut depth = 0;
    for b in s.bytes() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            b',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// Walk backwards from `fn_start` (the byte index of `f` in `function`)
/// and capture the contiguous doc-comment block immediately above it.
/// Recognizes triple-slash `///` line comments and `/** ... */` block
/// comments. Blank lines between the doc block and `function` terminate
/// the capture (per NatSpec conventions).
fn extract_leading_doc_comment(source: &str, fn_start: usize) -> Option<String> {
    let prefix = &source[..fn_start];
    // Skip whitespace (but at most one blank line) right before the function.
    let mut end = prefix.len();
    let mut newlines = 0;
    while end > 0 {
        let c = prefix.as_bytes()[end - 1] as char;
        if c == '\n' {
            newlines += 1;
            if newlines > 1 {
                break;
            }
            end -= 1;
        } else if c.is_whitespace() {
            end -= 1;
        } else {
            break;
        }
    }
    let head = &prefix[..end];
    // Look at preceding lines: collect contiguous /// or /** */ lines.
    if head.trim_end().ends_with("*/") {
        // Block doc: find the start `/**`.
        let trimmed = head.trim_end();
        let block_end = trimmed.len();
        let start = trimmed.rfind("/**")?;
        let block = &trimmed[start + 3..block_end - 2];
        let cleaned = block
            .lines()
            .map(|l| l.trim().trim_start_matches('*').trim().to_string())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if cleaned.is_empty() {
            return None;
        }
        return Some(cleaned);
    }
    // Triple-slash lines: collect them.
    let mut collected: Vec<String> = Vec::new();
    let mut line_end = head.len();
    while line_end > 0 {
        let line_start = head[..line_end].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line = head[line_start..line_end].trim_end_matches('\r');
        let trimmed = line.trim_start();
        if trimmed.starts_with("///") {
            collected.push(trimmed.trim_start_matches("///").trim().to_string());
        } else if trimmed.is_empty() {
            // Allow a single blank inside the doc only when it's the immediate
            // separator we already permitted above. Treat further blanks as
            // terminator.
            break;
        } else {
            break;
        }
        if line_start == 0 {
            break;
        }
        line_end = line_start - 1;
    }
    if collected.is_empty() {
        None
    } else {
        collected.reverse();
        Some(collected.join("\n"))
    }
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn match_paren(s: &str, open_idx: usize) -> Option<usize> {
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

fn match_brace(s: &str, open_idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0;
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

/// Replace `//` and `/* */` comments with spaces. Byte offsets are
/// preserved (each comment byte → one space byte); multi-byte UTF-8
/// sequences in non-comment regions are copied verbatim. An earlier
/// version pushed `bytes[i] as char` which re-encoded non-ASCII bytes
/// as Latin-1 codepoints and corrupted offsets — see the matching
/// note in `super::natspec::strip_comments`.
fn strip_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = vec![0u8; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                out[i] = if bytes[i] == b'\n' { b'\n' } else { b' ' };
                i += 1;
            }
            if i + 1 < bytes.len() {
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
            }
        } else {
            out[i] = bytes[i];
            i += 1;
        }
    }
    String::from_utf8(out).expect("strip_comments preserves UTF-8")
}

/// Replace contents of `"..."` and `'...'` string literals with spaces
/// so a literal `assertEq` inside a revert message doesn't confuse the
/// scanner. Same byte-preserving discipline as [`strip_comments`].
fn strip_string_literals(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = vec![0u8; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            let quote = c;
            out[i] = c;
            i += 1;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    out[i] = b' ';
                    out[i + 1] = b' ';
                    i += 2;
                    continue;
                }
                out[i] = if bytes[i] == b'\n' { b'\n' } else { b' ' };
                i += 1;
            }
            if i < bytes.len() {
                out[i] = bytes[i];
                i += 1;
            }
        } else {
            out[i] = c;
            i += 1;
        }
    }
    String::from_utf8(out).expect("strip_string_literals preserves UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    fn write_fixture(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, body).unwrap();
    }

    // --- Source-level parsing ---

    #[test]
    fn parses_foundry_test_functions() {
        let src = indoc! {r#"
            // SPDX-License-Identifier: MIT
            pragma solidity ^0.8.20;

            import "forge-std/Test.sol";

            contract FooTest {
                /// transfer must not change totalSupply
                function testTransferPreservesSupply() public {
                    uint256 a = token.totalSupply();
                    token.transfer(bob, 5);
                    assertEq(token.totalSupply(), a);
                }

                function testExpectRevertOnZero() public {
                    vm.expectRevert();
                    token.transfer(address(0), 1);
                }

                function testTrueAndFalse() public {
                    assertTrue(token.balanceOf(alice) > 0);
                    assertFalse(token.paused());
                }
            }
        "#};
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 3);

        let t0 = &tests[0];
        assert_eq!(t0.name, "testTransferPreservesSupply");
        assert_eq!(
            t0.doc_comment.as_deref(),
            Some("transfer must not change totalSupply")
        );
        assert!(
            t0.assertions
                .iter()
                .any(|a| matches!(a, Assertion::Eq { lhs, rhs }
                    if lhs.contains("totalSupply") && rhs == "a")),
            "expected assertEq(totalSupply(), a); got {:?}",
            t0.assertions
        );

        let t1 = &tests[1];
        assert_eq!(t1.name, "testExpectRevertOnZero");
        assert!(
            t1.assertions
                .iter()
                .any(|a| matches!(a, Assertion::ExpectRevert)),
            "expected ExpectRevert in {:?}",
            t1.assertions
        );

        let t2 = &tests[2];
        assert_eq!(t2.assertions.len(), 2);
        assert!(
            matches!(&t2.assertions[0], Assertion::True { expr } if expr.contains("balanceOf"))
        );
        assert!(matches!(&t2.assertions[1], Assertion::False { expr } if expr.contains("paused")));
    }

    #[test]
    fn captures_halmos_check_functions() {
        // Mirrors examples/vault-4626/test/Properties.t.sol shape.
        let src = indoc! {r#"
            pragma solidity ^0.8.20;
            contract Properties {
                /// More input assets convert to at least as many shares — never fewer.
                function check_convertToShares_is_monotone(uint256 a1, uint256 a2) external view {
                    require(a1 <= a2);
                    assert(token.convertToShares(a1) <= token.convertToShares(a2));
                }
            }
        "#};
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 1);
        let t = &tests[0];
        assert_eq!(t.name, "check_convertToShares_is_monotone");
        assert_eq!(
            t.doc_comment.as_deref(),
            Some("More input assets convert to at least as many shares — never fewer.")
        );
        assert!(
            t.assertions
                .iter()
                .any(|a| matches!(a, Assertion::HalmosAssert { expr } if expr.contains("convertToShares"))),
            "want HalmosAssert(convertToShares...); got {:?}",
            t.assertions
        );
    }

    #[test]
    fn skips_non_test_functions() {
        // Helpers and the constructor must not appear as tests.
        let src = indoc! {r#"
            contract Foo {
                constructor() {}
                function setUp() public {}
                function helper(uint256 x) internal pure returns (uint256) { return x; }
                function testReal() public { assertEq(1, 1); }
            }
        "#};
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "testReal");
    }

    #[test]
    fn handles_multi_line_assert_eq() {
        let src = indoc! {r#"
            contract C {
                function testWide() public {
                    assertEq(
                        token.totalSupply(),
                        before + amount
                    );
                }
            }
        "#};
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 1);
        match &tests[0].assertions[0] {
            Assertion::Eq { lhs, rhs } => {
                assert!(lhs.contains("totalSupply"));
                assert!(rhs.contains("before + amount"));
            }
            other => panic!("expected Eq, got {other:?}"),
        }
    }

    #[test]
    fn missing_doc_comment_yields_none() {
        let src = indoc! {r#"
            contract C {
                function testBare() public { assertTrue(true); }
            }
        "#};
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 1);
        assert!(tests[0].doc_comment.is_none());
    }

    #[test]
    fn captures_block_doc_comment() {
        let src = indoc! {r#"
            contract C {
                /** @notice this is a block-form natspec
                 *  spanning two lines.
                 */
                function testBlockDoc() public { assertEq(1, 1); }
            }
        "#};
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 1);
        let dc = tests[0].doc_comment.as_deref().unwrap();
        assert!(dc.contains("block-form natspec"));
        assert!(dc.contains("spanning two lines"));
    }

    #[test]
    fn ignores_commented_out_function() {
        let src = indoc! {r#"
            contract C {
                // function testGhost() public { assertEq(1, 2); }
                /* function testAlsoGhost() public { assertEq(1, 2); } */
                function testReal() public { assertEq(1, 1); }
            }
        "#};
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "testReal");
    }

    #[test]
    fn assertion_inside_string_literal_is_not_captured() {
        let src = indoc! {r#"
            contract C {
                function testRevertMessage() public {
                    require(false, "assertEq is not an assertion here");
                    assertEq(1, 1);
                }
            }
        "#};
        let tests = parse_tests_from_source(src);
        let assertions = &tests[0].assertions;
        assert_eq!(
            assertions.len(),
            1,
            "string-literal assertEq should be skipped; got {assertions:?}"
        );
    }

    #[test]
    fn bare_assert_distinct_from_assert_eq() {
        let src = indoc! {r#"
            contract C {
                function check_bare() public {
                    assert(x == y);
                    assertEq(x, y);
                }
            }
        "#};
        let tests = parse_tests_from_source(src);
        let a = &tests[0].assertions;
        assert_eq!(a.len(), 2);
        assert!(matches!(&a[0], Assertion::HalmosAssert { expr } if expr == "x == y"));
        assert!(matches!(&a[1], Assertion::Eq { .. }));
    }

    // --- Filesystem-level parsing ---

    #[test]
    fn parse_tests_walks_foundry_test_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_fixture(
            root,
            "test/Foo.t.sol",
            indoc! {r#"
                contract Foo {
                    /// preserves supply
                    function testA() public { assertEq(1, 1); }
                }
            "#},
        );
        write_fixture(
            root,
            "test/Bar.t.sol",
            indoc! {r#"
                contract Bar {
                    function testB() public { assertTrue(true); }
                }
            "#},
        );
        let tests = parse_tests(root).unwrap();
        assert_eq!(tests.len(), 2);
        let names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"testA"));
        assert!(names.contains(&"testB"));
        // source_path is set per fixture.
        assert!(tests.iter().all(|t| !t.source_path.as_os_str().is_empty()));
    }

    #[test]
    fn parse_tests_errors_on_non_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("not-a-dir.sol");
        std::fs::write(&f, "").unwrap();
        let err = parse_tests(&f).unwrap_err();
        assert!(matches!(err, TestParserError::NotADirectory(_)));
    }

    #[test]
    fn parse_tests_warns_but_does_not_crash_on_hardhat_layout() {
        // package.json with "hardhat" listed → warning + empty result (no
        // .t.sol files present).
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_fixture(
            root,
            "package.json",
            r#"{ "devDependencies": { "hardhat": "^2.0.0" } }"#,
        );
        // No solidity tests on disk — we only verify the call succeeds and
        // returns an empty result, not a crash.
        let tests = parse_tests(root).unwrap();
        assert!(tests.is_empty());
    }

    #[test]
    fn parses_real_reference_example_vault_4626() {
        // Phase 4 Slice 1 acceptance: the parser handles the exact shape of
        // our reference Halmos test files. vault-4626's Properties.t.sol has
        // four check_ functions, each with a leading /// doc comment.
        let src = include_str!("../../../examples/vault-4626/test/Properties.t.sol");
        let tests = parse_tests_from_source(src);
        assert_eq!(tests.len(), 4, "expected 4 check_ functions; got {tests:?}");
        for t in &tests {
            assert!(t.name.starts_with("check_"), "unexpected name {}", t.name);
            assert!(t.doc_comment.is_some(), "{} missing doc comment", t.name);
            assert!(
                t.assertions
                    .iter()
                    .any(|a| matches!(a, Assertion::HalmosAssert { .. })),
                "{} has no HalmosAssert: {:?}",
                t.name,
                t.assertions
            );
        }
    }
}
