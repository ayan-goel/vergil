//! Extract NatSpec doc-comment tags from Solidity source for Phase 4
//! §3.4b. The downstream LLM uses these as candidate-property seeds.
//!
//! Recognizes both `///` line and `/** ... */` block doc-comment forms.
//! Captures `@notice`, `@dev`, `@invariant` (first-class per SPEC §3.4b
//! — strongest intent signal), and `@custom:security`. Other tags
//! (`@param`, `@return`, `@author`, `@title`) are ignored. A free-floating
//! doc block not attached to a contract / function / storage declaration
//! is dropped — comments inside function bodies are noise, not intent.
//!
//! Same regex-y philosophy as [`super::signatures`] and
//! [`super::test_parser`]. Phase 4 intentionally avoids pulling in a full
//! Solidity parser; the LLM is robust to coarse signal.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// One NatSpec doc-comment block attached to a contract / function /
/// storage declaration. `notice` collapses to a single string (NatSpec
/// allows only one), while `dev` / `invariant` / `custom_security` are
/// vectors because real contracts often stack multiple tags of the same
/// kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NatSpecBlock {
    pub target: NatSpecTarget,
    pub notice: Option<String>,
    pub dev: Vec<String>,
    /// `@invariant ...` — first-class per SPEC §3.4b. The strongest
    /// intent signal because the author has explicitly named an
    /// always-true property; the LLM treats these as direct property
    /// statements with minimal reinterpretation.
    pub invariant: Vec<String>,
    /// `@custom:security ...` — treated like `@invariant` (strong
    /// signal of an intentional invariant, often used by audited
    /// contracts to encode security properties).
    pub custom_security: Vec<String>,
    pub source_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NatSpecTarget {
    Contract {
        name: String,
    },
    /// Includes free functions and modifiers; the LLM treats them
    /// uniformly. Constructor / receive / fallback are skipped (they
    /// rarely carry useful intent for property extraction).
    Function {
        name: String,
    },
    /// A storage variable. NatSpec on these is rare but real (e.g.,
    /// `/// @invariant totalSupply equals sum of balances` above a
    /// `uint256 public totalSupply` slot).
    Storage {
        name: String,
    },
}

/// Byte-offset span of the doc-comment block in the original source.
/// Used by future Phase 4 work for traceback messages when an LLM
/// candidate is rejected — the user can see exactly which doc comment
/// drove the (rejected) suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Error)]
pub enum NatSpecParserError {
    #[error("not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Parse NatSpec from a single Solidity source string. Order is the
/// same as appearance in the file. Doc comments without an attached
/// declaration are not returned (per acceptance criterion 2).
pub fn parse_natspec(source: &str) -> Vec<NatSpecBlock> {
    let mut out = Vec::new();
    // Scan for `function ...`, `contract ...`, `library ...`, `interface ...`,
    // and recognizable storage-variable declarations at the start of a line.
    // Each candidate site is fed `extract_block_above` to walk the preceding
    // doc comment (if any) and decide whether to emit a NatSpecBlock.
    for site in find_declaration_sites(source) {
        if let Some(block) = extract_block_above(source, &site) {
            out.push(block);
        }
    }
    out
}

/// Walk a project root's `src/` directory and extract NatSpec from every
/// `.sol` file. Excludes `lib/`, `test/`, `tests/`, and `script/` —
/// per SPEC §3.4b open question 3, V1.5 only extracts from the project's
/// own contracts, not from inherited OpenZeppelin / forge-std imports.
pub fn parse_natspec_dir(
    project_root: &Path,
) -> Result<Vec<(PathBuf, NatSpecBlock)>, NatSpecParserError> {
    if !project_root.is_dir() {
        return Err(NatSpecParserError::NotADirectory(
            project_root.to_path_buf(),
        ));
    }
    let src_dir = project_root.join("src");
    let mut files = Vec::new();
    if src_dir.is_dir() {
        collect_sol_files(&src_dir, &mut files)?;
    } else {
        // Single-file layout: scan the project root itself (shallow), but
        // skip anything under test/ tests/ lib/ script/ out/ cache/.
        collect_sol_files_shallow(project_root, &mut files)?;
    }
    files.sort();
    let mut out = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).map_err(|e| NatSpecParserError::Io {
            path: path.clone(),
            source: e,
        })?;
        let rel = path
            .strip_prefix(project_root)
            .unwrap_or(path)
            .to_path_buf();
        for block in parse_natspec(&src) {
            out.push((rel.clone(), block));
        }
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct DeclarationSite {
    /// Byte index of the first character of the declaration keyword
    /// (`contract`, `function`, etc.) or the storage type name.
    start: usize,
    target: NatSpecTarget,
}

fn find_declaration_sites(source: &str) -> Vec<DeclarationSite> {
    let blanked = strip_comments(source);
    let bytes = blanked.as_bytes();
    let mut out = Vec::new();
    // First: contracts / libraries / interfaces.
    for keyword in ["contract ", "library ", "interface ", "abstract contract "] {
        let mut from = 0;
        while let Some(rel) = blanked[from..].find(keyword) {
            let abs = from + rel;
            if is_token_start(&blanked, abs) {
                let after = abs + keyword.len();
                if let Some(name) = read_ident(&blanked, after) {
                    out.push(DeclarationSite {
                        start: abs,
                        target: NatSpecTarget::Contract { name },
                    });
                }
            }
            from = abs + keyword.len();
        }
    }
    // Then functions.
    let mut from = 0;
    while let Some(rel) = blanked[from..].find("function ") {
        let abs = from + rel;
        if is_token_start(&blanked, abs) {
            let after = abs + "function ".len();
            if let Some(name) = read_ident(&blanked, after) {
                if name != "constructor" && name != "receive" && name != "fallback" {
                    out.push(DeclarationSite {
                        start: abs,
                        target: NatSpecTarget::Function { name },
                    });
                }
            }
        }
        from = abs + "function ".len();
    }
    // Then storage declarations: simple heuristic — a doc comment
    // followed by a type token and `public`/`private`/`internal`/`constant`/
    // `immutable` and a name, at depth 0 inside a contract block. We
    // approximate "depth 0" by scanning for lines beginning a declaration
    // pattern within the contract body. This is best-effort; missing a
    // storage NatSpec block is a soft fail (the LLM still has the
    // function-level + contract-level signals).
    for site in find_storage_sites(&blanked) {
        out.push(site);
    }
    let _ = bytes;
    out.sort_by_key(|s| s.start);
    out
}

fn find_storage_sites(blanked: &str) -> Vec<DeclarationSite> {
    let mut out = Vec::new();
    for (line_start, line) in line_offsets(blanked) {
        let trimmed = line.trim_start();
        // Must start with a Solidity type-ish token. We accept the
        // built-in primitives and a generic identifier (for user types).
        let (type_tok, rest) = split_ident(trimmed);
        if type_tok.is_empty() {
            continue;
        }
        if !is_storage_first_token(&type_tok) {
            continue;
        }
        // The line must contain one of the visibility/storage modifiers
        // (so we don't mistake a return-type declaration inside a
        // function for a storage slot).
        if !rest.contains(" public ")
            && !rest.contains(" private ")
            && !rest.contains(" internal ")
            && !rest.contains(" constant ")
            && !rest.contains(" immutable ")
            && !rest.contains("public ")
            && !rest.contains("private ")
            && !rest.contains("internal ")
        {
            // Be permissive: accept lines that look like `mapping(...) public foo;`
            // which the test above may miss; require either `public`/`private`/
            // `internal` OR an ending `;` on the same line PLUS no `{`.
            if !line.contains(';') || line.contains('{') {
                continue;
            }
            if !line.contains("public")
                && !line.contains("private")
                && !line.contains("internal")
                && !line.contains("constant")
                && !line.contains("immutable")
            {
                continue;
            }
        }
        // Pull the identifier just before the `;` or the first `=` — that's
        // the storage name. This is regex-y; it's fine for the heuristic.
        let Some(name) = extract_storage_name(line) else {
            continue;
        };
        out.push(DeclarationSite {
            start: line_start + (line.len() - trimmed.len()),
            target: NatSpecTarget::Storage { name },
        });
    }
    out
}

fn is_storage_first_token(t: &str) -> bool {
    matches!(
        t,
        "uint256"
            | "uint128"
            | "uint64"
            | "uint32"
            | "uint16"
            | "uint8"
            | "uint"
            | "int256"
            | "int128"
            | "int"
            | "address"
            | "bool"
            | "bytes32"
            | "bytes"
            | "string"
            | "mapping"
    ) || t.starts_with(|c: char| c.is_ascii_uppercase()) // user types: ERC20, MyEnum, etc.
}

fn extract_storage_name(line: &str) -> Option<String> {
    // Strip trailing `;` and any initializer.
    let body = line.split(';').next()?;
    let body = body.split('=').next()?;
    // The last identifier on the line is the storage name.
    let mut last = None;
    let mut buf = String::new();
    for c in body.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            buf.push(c);
        } else if !buf.is_empty() {
            last = Some(buf.clone());
            buf.clear();
        }
    }
    if !buf.is_empty() {
        last = Some(buf);
    }
    let name = last?;
    if matches!(
        name.as_str(),
        "public" | "private" | "internal" | "constant" | "immutable" | "memory" | "storage"
    ) {
        return None;
    }
    Some(name)
}

fn line_offsets(s: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = 0;
    let bytes = s.as_bytes();
    std::iter::from_fn(move || {
        if start > bytes.len() {
            return None;
        }
        let from = start;
        let mut end = from;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        let line = &s[from..end];
        start = end + 1;
        if from > bytes.len() {
            None
        } else {
            Some((from, line))
        }
    })
}

fn split_ident(s: &str) -> (String, &str) {
    let mut end = 0;
    let bytes = s.as_bytes();
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    (s[..end].to_string(), &s[end..])
}

fn is_token_start(s: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }
    let prev = s.as_bytes()[at - 1] as char;
    !is_ident(prev)
}

fn read_ident(s: &str, at: usize) -> Option<String> {
    let bytes = s.as_bytes();
    let mut k = at;
    while k < bytes.len() && (bytes[k] as char).is_whitespace() {
        k += 1;
    }
    let start = k;
    while k < bytes.len() && is_ident(bytes[k] as char) {
        k += 1;
    }
    if k == start {
        None
    } else {
        Some(s[start..k].to_string())
    }
}

/// Walk backward from a declaration site and extract any contiguous
/// NatSpec block (`///` or `/** */`). Returns None when there is no
/// doc comment above, or when the doc block carries no recognized
/// tags (and no implicit notice text).
fn extract_block_above(source: &str, site: &DeclarationSite) -> Option<NatSpecBlock> {
    let prefix = &source[..site.start];
    let mut end = prefix.len();
    // Allow at most one blank line between the doc block and the decl.
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
    let trimmed = head.trim_end();
    // Try block form first: `/** ... */`.
    if trimmed.ends_with("*/") {
        let block_end = trimmed.len();
        let start = trimmed.rfind("/**")?;
        let block_body = &trimmed[start + 3..block_end - 2];
        let lines: Vec<String> = block_body
            .lines()
            .map(|l| l.trim().trim_start_matches('*').trim().to_string())
            .collect();
        let span = SourceSpan {
            start,
            end: block_end,
        };
        return finalize_block(&lines, site.target.clone(), span);
    }
    // Triple-slash form: collect contiguous `///` lines.
    let mut collected: Vec<String> = Vec::new();
    let mut collected_start = head.len();
    let mut line_end = head.len();
    while line_end > 0 {
        let line_start = head[..line_end].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line = head[line_start..line_end].trim_end_matches('\r');
        let trimmed_line = line.trim_start();
        if trimmed_line.starts_with("///") {
            collected.push(trimmed_line.trim_start_matches("///").trim().to_string());
            collected_start = line_start;
        } else if trimmed_line.is_empty() {
            // Allow one blank above the doc block, but inside the doc block
            // a blank line terminates.
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
        return None;
    }
    collected.reverse();
    let span = SourceSpan {
        start: collected_start,
        end: head.len(),
    };
    finalize_block(&collected, site.target.clone(), span)
}

fn finalize_block(
    lines: &[String],
    target: NatSpecTarget,
    source_span: SourceSpan,
) -> Option<NatSpecBlock> {
    let mut notice: Option<String> = None;
    let mut dev: Vec<String> = Vec::new();
    let mut invariant: Vec<String> = Vec::new();
    let mut custom_security: Vec<String> = Vec::new();
    let mut implicit_notice_lines: Vec<String> = Vec::new();
    let mut current_tag: Option<TagKind> = None;
    let mut current_buf: Vec<String> = Vec::new();

    fn flush(
        tag: &mut Option<TagKind>,
        buf: &mut Vec<String>,
        notice: &mut Option<String>,
        dev: &mut Vec<String>,
        invariant: &mut Vec<String>,
        custom_security: &mut Vec<String>,
    ) {
        if let Some(t) = tag.take() {
            let joined = buf.join("\n").trim().to_string();
            buf.clear();
            if joined.is_empty() {
                return;
            }
            match t {
                TagKind::Notice => *notice = Some(joined),
                TagKind::Dev => dev.push(joined),
                TagKind::Invariant => invariant.push(joined),
                TagKind::CustomSecurity => custom_security.push(joined),
            }
        }
    }

    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = strip_tag(trimmed, "@notice") {
            flush(
                &mut current_tag,
                &mut current_buf,
                &mut notice,
                &mut dev,
                &mut invariant,
                &mut custom_security,
            );
            current_tag = Some(TagKind::Notice);
            current_buf.push(rest.to_string());
            continue;
        }
        if let Some(rest) = strip_tag(trimmed, "@dev") {
            flush(
                &mut current_tag,
                &mut current_buf,
                &mut notice,
                &mut dev,
                &mut invariant,
                &mut custom_security,
            );
            current_tag = Some(TagKind::Dev);
            current_buf.push(rest.to_string());
            continue;
        }
        if let Some(rest) = strip_tag(trimmed, "@invariant") {
            flush(
                &mut current_tag,
                &mut current_buf,
                &mut notice,
                &mut dev,
                &mut invariant,
                &mut custom_security,
            );
            current_tag = Some(TagKind::Invariant);
            current_buf.push(rest.to_string());
            continue;
        }
        if let Some(rest) = strip_tag(trimmed, "@custom:security") {
            flush(
                &mut current_tag,
                &mut current_buf,
                &mut notice,
                &mut dev,
                &mut invariant,
                &mut custom_security,
            );
            current_tag = Some(TagKind::CustomSecurity);
            current_buf.push(rest.to_string());
            continue;
        }
        // Other tags (`@param`, `@return`, `@author`, `@title`, etc.)
        // terminate the current tag.
        if trimmed.starts_with('@') {
            flush(
                &mut current_tag,
                &mut current_buf,
                &mut notice,
                &mut dev,
                &mut invariant,
                &mut custom_security,
            );
            current_tag = None;
            continue;
        }
        // Continuation line for the current tag, or implicit-notice text
        // (no tag yet seen).
        if current_tag.is_some() {
            current_buf.push(trimmed.to_string());
        } else {
            implicit_notice_lines.push(trimmed.to_string());
        }
    }
    flush(
        &mut current_tag,
        &mut current_buf,
        &mut notice,
        &mut dev,
        &mut invariant,
        &mut custom_security,
    );
    // Untagged leading lines become the implicit `@notice` text per
    // NatSpec convention. Only adopt them if there's no explicit notice
    // already, to avoid clobbering an author's `@notice` choice.
    if notice.is_none() && !implicit_notice_lines.is_empty() {
        notice = Some(implicit_notice_lines.join("\n"));
    }
    if notice.is_none() && dev.is_empty() && invariant.is_empty() && custom_security.is_empty() {
        return None;
    }
    Some(NatSpecBlock {
        target,
        notice,
        dev,
        invariant,
        custom_security,
        source_span,
    })
}

#[derive(Debug, Clone, Copy)]
enum TagKind {
    Notice,
    Dev,
    Invariant,
    CustomSecurity,
}

fn strip_tag<'a>(line: &'a str, tag: &str) -> Option<&'a str> {
    if !line.starts_with(tag) {
        return None;
    }
    let after = &line[tag.len()..];
    // The tag must be followed by whitespace, end of line, or nothing.
    if after.is_empty() {
        return Some("");
    }
    let first = after.chars().next().unwrap();
    if first.is_whitespace() || first == ':' {
        // `:` covers `@custom:security` already handled, but tolerate
        // a stray colon after a tag.
        return Some(after.trim_start_matches(':').trim_start());
    }
    None
}

fn collect_sol_files(dir: &Path, into: &mut Vec<PathBuf>) -> Result<(), NatSpecParserError> {
    let entries = std::fs::read_dir(dir).map_err(|e| NatSpecParserError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| NatSpecParserError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        if path.is_dir() {
            // Skip nested vendored dirs that look like inherited libraries.
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if matches!(name, "lib" | "node_modules" | "out" | "cache" | "broadcast")
                    || name.starts_with('.')
                {
                    continue;
                }
            }
            collect_sol_files(&path, into)?;
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) == Some("sol") {
            into.push(path);
        }
    }
    Ok(())
}

fn collect_sol_files_shallow(
    dir: &Path,
    into: &mut Vec<PathBuf>,
) -> Result<(), NatSpecParserError> {
    let entries = std::fs::read_dir(dir).map_err(|e| NatSpecParserError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| NatSpecParserError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("sol") {
            into.push(path);
        }
    }
    Ok(())
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Replace non-doc comments with spaces so structural scans see
/// consistent byte offsets. Doc comments (`///`, `/** */`) are kept
/// because the caller distinguishes them in [`extract_block_above`].
///
/// Non-comment bytes are copied verbatim so multi-byte UTF-8 sequences
/// (e.g. an em-dash `—` inside a doc comment) survive intact and byte
/// offsets in the returned string match those in `source`. An earlier
/// version of this used `out.push(bytes[i] as char)` which re-encoded
/// non-ASCII bytes as Latin-1 codepoints and inflated the string —
/// the bug that broke block-form NatSpec extraction.
fn strip_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = vec![0u8; bytes.len()];
    let mut i = 0;
    while i < bytes.len() {
        if i + 2 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' && bytes[i + 2] != b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
            continue;
        }
        if i + 2 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' && bytes[i + 2] != b'*' {
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
            continue;
        }
        out[i] = bytes[i];
        i += 1;
    }
    // Safe: we only ever replaced ASCII bytes with space/newline. The
    // multi-byte UTF-8 sequences were copied verbatim, so the result is
    // valid UTF-8 if the input was.
    String::from_utf8(out).expect("strip_comments preserves UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn captures_line_form_notice_dev_on_function() {
        let src = indoc! {r#"
            contract Token {
                /// @notice transfer moves tokens from caller to recipient
                /// @dev assumes the caller has approved sufficient allowance
                function transfer(address to, uint256 amount) external returns (bool) {}
            }
        "#};
        let blocks = parse_natspec(src);
        let tf = blocks
            .iter()
            .find(|b| matches!(&b.target, NatSpecTarget::Function { name } if name == "transfer"))
            .expect("function transfer block not found");
        assert_eq!(
            tf.notice.as_deref(),
            Some("transfer moves tokens from caller to recipient")
        );
        assert_eq!(tf.dev.len(), 1);
        assert!(tf.dev[0].contains("approved sufficient allowance"));
    }

    #[test]
    fn captures_block_form_with_multi_line_tag() {
        let src = indoc! {r#"
            contract C {
                /**
                 * @notice deposits underlying assets and mints shares
                 *         to the receiver.
                 * @dev rounding is toward floor — see ERC-4626
                 *      preview semantics.
                 */
                function deposit(uint256 assets, address receiver) external returns (uint256) {}
            }
        "#};
        let blocks = parse_natspec(src);
        let b = blocks
            .iter()
            .find(|b| matches!(&b.target, NatSpecTarget::Function { name } if name == "deposit"))
            .unwrap();
        let notice = b.notice.as_deref().unwrap();
        assert!(notice.contains("deposits underlying"));
        assert!(notice.contains("to the receiver"));
        assert_eq!(b.dev.len(), 1);
        assert!(b.dev[0].contains("rounding is toward floor"));
        assert!(b.dev[0].contains("preview semantics"));
    }

    #[test]
    fn invariant_is_first_class_field() {
        // Per SPEC §3.4b: `@invariant` is the strongest intent signal.
        let src = indoc! {r#"
            /// @notice total supply tracks share emissions
            /// @invariant totalSupply == sum(balanceOf[*])
            /// @invariant totalAssets >= sum(assetBalance[*])
            contract Vault {}
        "#};
        let blocks = parse_natspec(src);
        let b = blocks
            .iter()
            .find(|b| matches!(&b.target, NatSpecTarget::Contract { name } if name == "Vault"))
            .unwrap();
        assert_eq!(b.invariant.len(), 2);
        assert!(b.invariant[0].contains("totalSupply == sum"));
        assert!(b.invariant[1].contains("totalAssets >= sum"));
    }

    #[test]
    fn custom_security_tag_is_captured() {
        let src = indoc! {r#"
            contract C {
                /// @custom:security non-reentrant via OZ ReentrancyGuard
                /// @custom:security only owner can call
                function adminWithdraw() external {}
            }
        "#};
        let blocks = parse_natspec(src);
        let b = &blocks[0];
        assert_eq!(b.custom_security.len(), 2);
        assert!(b.custom_security[0].contains("non-reentrant"));
        assert!(b.custom_security[1].contains("only owner"));
    }

    #[test]
    fn untagged_doc_becomes_implicit_notice() {
        // Per NatSpec convention, a leading untagged line is treated as @notice.
        let src = indoc! {r#"
            contract C {
                /// Burns `amount` tokens from the caller's balance.
                /// Reverts when the caller has insufficient balance.
                function burn(uint256 amount) external {}
            }
        "#};
        let blocks = parse_natspec(src);
        let n = blocks[0].notice.as_deref().unwrap();
        assert!(n.contains("Burns `amount`"));
        assert!(n.contains("Reverts when"));
    }

    #[test]
    fn floating_comment_inside_body_is_ignored() {
        // Doc comments not attached to a declaration must be dropped
        // (acceptance criterion 2).
        let src = indoc! {r#"
            contract C {
                function f() public {
                    /// @notice this comment is dangling inside a body
                    uint256 x = 1;
                }
            }
        "#};
        let blocks = parse_natspec(src);
        // Only the `function f` declaration site is considered, and it
        // has no doc comment above it.
        assert!(blocks.iter().all(|b| b.notice.is_none()
            && b.dev.is_empty()
            && b.invariant.is_empty()
            && b.custom_security.is_empty()));
        // The block list may be empty or may contain f with no fields —
        // either way nothing extracted from the dangling comment.
        let dangling = blocks
            .iter()
            .any(|b| b.notice.as_deref().unwrap_or("").contains("dangling"));
        assert!(!dangling, "dangling-body NatSpec leaked: {blocks:?}");
    }

    #[test]
    fn unknown_tags_terminate_capture_but_dont_corrupt() {
        let src = indoc! {r#"
            contract C {
                /// @notice transfers `amount` tokens
                /// @param to the recipient
                /// @return success
                function transfer(address to, uint256 amount) external returns (bool) {}
            }
        "#};
        let blocks = parse_natspec(src);
        let b = &blocks[0];
        assert_eq!(
            b.notice.as_deref(),
            Some("transfers `amount` tokens"),
            "@param should not append to @notice"
        );
        // The other tags are ignored — they're not in our output.
        assert!(b.dev.is_empty());
        assert!(b.invariant.is_empty());
    }

    #[test]
    fn contract_block_attaches_to_contract_keyword() {
        let src = indoc! {r#"
            /// @notice fungible token implementation
            /// @dev minimal ERC-20 — no permit, no pausable
            contract MyToken {
                function f() public {}
            }
        "#};
        let blocks = parse_natspec(src);
        let mt = blocks
            .iter()
            .find(|b| matches!(&b.target, NatSpecTarget::Contract { name } if name == "MyToken"))
            .unwrap();
        assert_eq!(mt.notice.as_deref(), Some("fungible token implementation"));
        assert_eq!(mt.dev.len(), 1);
    }

    #[test]
    fn storage_slot_natspec_is_captured() {
        let src = indoc! {r#"
            contract C {
                /// @invariant totalSupply == sum(balanceOf[*])
                uint256 public totalSupply;
            }
        "#};
        let blocks = parse_natspec(src);
        let ts = blocks.iter().find(
            |b| matches!(&b.target, NatSpecTarget::Storage { name } if name == "totalSupply"),
        );
        assert!(
            ts.is_some(),
            "totalSupply storage block missing: {blocks:?}"
        );
        let ts = ts.unwrap();
        assert_eq!(ts.invariant.len(), 1);
        assert!(ts.invariant[0].contains("sum(balanceOf"));
    }

    #[test]
    fn block_with_no_recognized_tags_returns_none() {
        let src = indoc! {r#"
            contract C {
                /// @author satoshi
                /// @param x irrelevant
                function f(uint256 x) external {}
            }
        "#};
        let blocks = parse_natspec(src);
        // f has no recognized tag → no block returned at all (or one
        // with all fields empty, which we forbid in finalize_block).
        assert!(
            blocks
                .iter()
                .all(|b| !matches!(&b.target, NatSpecTarget::Function { name } if name == "f")),
            "uninteresting block leaked: {blocks:?}"
        );
    }

    #[test]
    fn multiple_blocks_keyed_by_declaration() {
        let src = indoc! {r#"
            /// @notice the vault
            contract Vault {
                /// @invariant totalSupply tracks shares
                uint256 public totalSupply;

                /// @notice deposits underlying
                function deposit(uint256 a) external {}

                /// @notice redeems shares
                /// @dev rounds down
                function redeem(uint256 s) external {}
            }
        "#};
        let blocks = parse_natspec(src);
        let names: Vec<String> = blocks
            .iter()
            .map(|b| match &b.target {
                NatSpecTarget::Contract { name } => format!("C:{name}"),
                NatSpecTarget::Function { name } => format!("F:{name}"),
                NatSpecTarget::Storage { name } => format!("S:{name}"),
            })
            .collect();
        assert!(names.contains(&"C:Vault".to_string()), "{names:?}");
        assert!(names.contains(&"F:deposit".to_string()), "{names:?}");
        assert!(names.contains(&"F:redeem".to_string()), "{names:?}");
        assert!(names.contains(&"S:totalSupply".to_string()), "{names:?}");
    }

    #[test]
    fn source_span_points_into_original_text() {
        let src = "/// @notice a token\ncontract Foo {}\n";
        let blocks = parse_natspec(src);
        let b = &blocks[0];
        // The span captures the doc-comment block, not the declaration.
        let captured = &src[b.source_span.start..b.source_span.end];
        assert!(captured.contains("@notice"), "span text: {captured:?}");
    }

    // --- Filesystem layer ---

    #[test]
    fn parse_natspec_dir_only_reads_src_when_present() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::create_dir_all(root.join("test")).unwrap();
        std::fs::write(
            root.join("src/Token.sol"),
            "/// @notice my token\ncontract Token {}\n",
        )
        .unwrap();
        // Inherited library NatSpec — must NOT be returned.
        std::fs::write(
            root.join("lib/OpenZeppelin.sol"),
            "/// @notice oz erc20\ncontract OZERC20 {}\n",
        )
        .unwrap();
        // Test files — must NOT be scanned by natspec_dir (the test
        // parser handles them).
        std::fs::write(
            root.join("test/Properties.t.sol"),
            "/// @notice property check\ncontract Properties {}\n",
        )
        .unwrap();
        let results = parse_natspec_dir(root).unwrap();
        let contract_names: Vec<&str> = results
            .iter()
            .filter_map(|(_p, b)| match &b.target {
                NatSpecTarget::Contract { name } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(contract_names, vec!["Token"]);
    }

    #[test]
    fn parse_natspec_dir_errors_on_non_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("not-a-dir");
        std::fs::write(&f, "").unwrap();
        let err = parse_natspec_dir(&f).unwrap_err();
        assert!(matches!(err, NatSpecParserError::NotADirectory(_)));
    }
}
