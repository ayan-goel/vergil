//! Extract function signatures from Solidity source. Phase 4 Slice A3.
//!
//! Used by the synthesizer to give the LLM an explicit list of
//! "available methods" — fixes the failure mode where the LLM proposes
//! candidates calling methods the contract doesn't expose (e.g.,
//! `isNonexistent`, `mulDivFloor` from Phase 3) or reaches for the
//! wrong interface entirely (ERC-20 shape for an ERC-721 contract).
//!
//! Intentionally a regex-y extractor, not a full Solidity parser. The
//! synth prompt only needs the signature shape (name + params + return);
//! a full parser would be overkill and add solang/solc as a dependency.

/// One Solidity function signature, suitable for an "available_methods"
/// block in the synth prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    /// Function name (e.g., `transfer`).
    pub name: String,
    /// Argument list as written, including parens (e.g., `(address to, uint256 amount)`).
    pub args: String,
    /// Visibility: `external`, `public`, `internal`, `private`.
    pub visibility: String,
    /// Return clause as written (e.g., `returns (bool)`), or empty.
    pub returns: String,
    /// `true` if the function is `view` / `pure` / `constant`.
    pub is_view_pure: bool,
}

impl FunctionSignature {
    /// Render in the shape the synth prompt embeds.
    pub fn render(&self) -> String {
        let mut s = format!("function {}{} {}", self.name, self.args, self.visibility);
        if self.is_view_pure {
            s.push_str(" view");
        }
        if !self.returns.is_empty() {
            s.push(' ');
            s.push_str(&self.returns);
        }
        s
    }
}

/// Extract every `external` / `public` function signature from Solidity
/// source. Skips `internal` / `private` (the synthesizer only cares about
/// the contract's external surface). Ignores constructors and modifiers.
pub fn extract(source: &str) -> Vec<FunctionSignature> {
    let stripped = strip_comments(source);
    let mut out = Vec::new();
    let bytes = stripped.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find the next `function ` token.
        let rest = &stripped[i..];
        let Some(rel) = rest.find("function ") else {
            break;
        };
        let start = i + rel;
        let after_kw = start + "function ".len();
        if after_kw >= bytes.len() {
            break;
        }
        // Name = identifier following `function `.
        let name_start = after_kw;
        let mut name_end = name_start;
        while name_end < bytes.len() && is_ident(bytes[name_end] as char) {
            name_end += 1;
        }
        let name = &stripped[name_start..name_end];
        // Skip constructors + receive/fallback — synthesizer doesn't call those.
        if name.is_empty() || name == "constructor" || name == "receive" || name == "fallback" {
            i = name_end;
            continue;
        }
        // Args: `(` ... matching `)`.
        let mut j = name_end;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'(' {
            i = j.max(start + 1);
            continue;
        }
        let Some(args_end) = match_paren(&stripped, j) else {
            i = j + 1;
            continue;
        };
        let args = &stripped[j..=args_end];
        // Modifiers + visibility + returns up to `{` or `;`.
        let mods_start = args_end + 1;
        let mut k = mods_start;
        let mut depth = 0;
        let mut block_end = mods_start;
        while k < bytes.len() {
            match bytes[k] {
                b'(' => depth += 1,
                b')' => {
                    if depth > 0 {
                        depth -= 1;
                    }
                }
                b'{' | b';' if depth == 0 => {
                    block_end = k;
                    break;
                }
                _ => {}
            }
            k += 1;
        }
        let mods = &stripped[mods_start..block_end];
        // Skip non-external functions.
        let visibility = if mods.contains("external") {
            "external"
        } else if mods.contains("public") {
            "public"
        } else {
            // Internal / private / unmarked — skip (synth shouldn't call these).
            i = block_end.max(start + 1) + 1;
            continue;
        };
        let is_view_pure = mods.contains(" view ")
            || mods.contains(" view\n")
            || mods.ends_with("view")
            || mods.contains(" pure ")
            || mods.contains(" pure\n")
            || mods.ends_with("pure");
        let returns = extract_returns(mods);
        out.push(FunctionSignature {
            name: name.to_string(),
            args: args.to_string(),
            visibility: visibility.to_string(),
            returns,
            is_view_pure,
        });
        i = block_end.max(start + 1) + 1;
    }
    out
}

/// Render the function signatures as the block the synth prompt embeds
/// under `{{ available_methods }}`. Empty string when no functions
/// extracted (the prompt's "available_methods" placeholder still works
/// — the LLM just falls back to reading the contract source directly).
pub fn render_available_methods(sigs: &[FunctionSignature]) -> String {
    if sigs.is_empty() {
        return "(no external functions detected — read the contract source above)".to_string();
    }
    sigs.iter()
        .map(|s| format!("- {}", s.render()))
        .collect::<Vec<_>>()
        .join("\n")
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

fn extract_returns(mods: &str) -> String {
    let Some(idx) = mods.find("returns") else {
        return String::new();
    };
    let after = &mods[idx + "returns".len()..];
    let trimmed = after.trim_start();
    if !trimmed.starts_with('(') {
        return String::new();
    }
    let Some(end) = match_paren(trimmed, 0) else {
        return String::new();
    };
    format!("returns {}", &trimmed[..=end])
}

/// Strip `//`-style and `/* */`-style comments so the regex-y extractor
/// doesn't trip on commented-out function declarations.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            // Line comment: skip to newline.
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Block comment: skip to `*/`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_external_view_function() {
        let src = r#"
            pragma solidity ^0.8.0;
            contract Foo {
                function balanceOf(address owner) external view returns (uint256) {
                    return 0;
                }
            }
        "#;
        let sigs = extract(src);
        assert_eq!(sigs.len(), 1);
        let s = &sigs[0];
        assert_eq!(s.name, "balanceOf");
        assert_eq!(s.args, "(address owner)");
        assert_eq!(s.visibility, "external");
        assert!(s.is_view_pure);
        assert_eq!(s.returns, "returns (uint256)");
    }

    #[test]
    fn skips_internal_and_private_functions() {
        let src = r#"
            contract Foo {
                function _burn(uint256 amount) internal { }
                function _secret() private { }
                function withdraw(uint256 amount) external { }
            }
        "#;
        let sigs = extract(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "withdraw");
    }

    #[test]
    fn skips_constructor_receive_fallback() {
        let src = r#"
            contract Foo {
                constructor(uint256 initial) public { }
                receive() external payable { }
                fallback() external payable { }
                function real() external { }
            }
        "#;
        let sigs = extract(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "real");
    }

    #[test]
    fn handles_multi_arg_functions() {
        let src = r#"
            contract Foo {
                function transferFrom(address from, address to, uint256 amount) external returns (bool) { }
            }
        "#;
        let sigs = extract(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].args, "(address from, address to, uint256 amount)");
        assert_eq!(sigs[0].returns, "returns (bool)");
    }

    #[test]
    fn handles_nested_parens_in_args() {
        // Solidity allows tuple types like `(uint256, uint256)` in returns.
        let src = r#"
            contract Foo {
                function burn(uint256 shares) external returns (uint256, uint256) { }
            }
        "#;
        let sigs = extract(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].returns, "returns (uint256, uint256)");
    }

    #[test]
    fn strips_comments_so_commented_function_is_ignored() {
        let src = r#"
            contract Foo {
                // function ghost() external { }
                /* function alsoGhost() external { } */
                function real() external { }
            }
        "#;
        let sigs = extract(src);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].name, "real");
    }

    #[test]
    fn render_available_methods_empty_input_returns_placeholder() {
        let s = render_available_methods(&[]);
        assert!(s.contains("no external functions"));
    }

    #[test]
    fn render_available_methods_lists_signatures_with_dash_prefix() {
        let sigs = extract(
            "contract C { function a() external {} function b(uint256 x) external view returns (uint256) {} }",
        );
        let rendered = render_available_methods(&sigs);
        assert!(rendered.starts_with("- function a"));
        assert!(rendered.contains("- function b"));
        assert!(rendered.contains("view"));
        assert!(rendered.contains("returns (uint256)"));
    }

    #[test]
    fn extracts_erc721_surface() {
        // The exact kind of contract Phase 3 stumbled on: ERC-721 with
        // approve, ownerOf, balanceOf, getApproved. With these in the
        // synth prompt, the LLM should stop reaching for ERC-20 shapes.
        let src = include_str!("../../../examples/erc721/src/ERC721.sol");
        let sigs = extract(src);
        let names: Vec<&str> = sigs.iter().map(|s| s.name.as_str()).collect();
        for required in [
            "ownerOf",
            "balanceOf",
            "getApproved",
            "approve",
            "setApprovalForAll",
            "transferFrom",
            "mint",
        ] {
            assert!(names.contains(&required), "missing {required} in {names:?}");
        }
        // The internal helper should be skipped.
        assert!(!names.contains(&"_isAuthorized"));
    }
}
