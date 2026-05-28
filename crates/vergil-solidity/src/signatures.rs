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

/// Classify a contract's interface(s) from its source, using the external
/// function surface (via [`extract`]) plus a few textual markers. Returns
/// interface tags drawn from the catalog's `applies_to.interfaces`
/// vocabulary (e.g. `ERC20`, `ERC721`, `ERC4626`, `Ownable`, `Pausable`).
///
/// Empty when nothing recognizable is found — callers treat the empty case
/// as "do not filter retrieval" so an unrecognized contract still gets the
/// full catalog (no worse than the pre-A1 behavior).
///
/// Detection is conservative and additive: a contract can carry several tags
/// (an ERC-20 that is also Pausable and Ownable yields all three). The one
/// hard exclusion that matters for synthesis quality is that an **ERC-721
/// does not get the ERC-20 tag** — ERC-20 templates dominate the catalog and
/// were the documented root cause of the ERC-721 kill-criterion stragglers
/// (`notes/phase4-a1-stragglers-diagnosis.md`).
pub fn detect_interfaces(source: &str) -> Vec<String> {
    let sigs = extract(source);
    let names: std::collections::HashSet<&str> = sigs.iter().map(|s| s.name.as_str()).collect();
    let has = |n: &str| names.contains(n);
    let text = strip_comments(source);

    let mut tags: Vec<String> = Vec::new();

    // --- token standards (mutually distinguishing) ---
    // ERC-721: non-fungible owner-tracking surface. The presence of
    // ownerOf + an approval-for-all/getApproved surface is what separates it
    // from ERC-20 (both share transfer/approve/balanceOf names).
    let is_erc721 =
        has("ownerOf") && (has("setApprovalForAll") || has("getApproved") || has("isApprovedForAll"));
    // ERC-4626: tokenized vault. Shares are themselves an ERC-20, so a vault
    // earns both tags.
    let is_erc4626 = has("asset")
        && (has("convertToShares")
            || has("convertToAssets")
            || has("totalAssets")
            || (has("deposit") && has("redeem")));
    // ERC-20: fungible allowance surface, and explicitly not an NFT.
    let is_erc20 = has("allowance") && has("transfer") && has("transferFrom") && !is_erc721;

    if is_erc721 {
        push_tag(&mut tags, "ERC721");
    }
    if is_erc4626 {
        push_tag(&mut tags, "ERC4626");
        push_tag(&mut tags, "ERC20");
    } else if is_erc20 {
        push_tag(&mut tags, "ERC20");
    }

    // --- common mixins (additive; over-inclusion is harmless) ---
    if has("transferOwnership") || has("owner") {
        push_tag(&mut tags, "Ownable");
    }
    if has("acceptOwnership") || has("pendingOwner") {
        push_tag(&mut tags, "Ownable2Step");
    }
    if has("paused") || text.contains("whenNotPaused") {
        push_tag(&mut tags, "Pausable");
    }
    if has("hasRole") && has("grantRole") {
        push_tag(&mut tags, "AccessControl");
    }
    if has("confirmTransaction")
        || has("executeTransaction")
        || has("submitTransaction")
        || (has("required") && text.contains("isOwner"))
    {
        push_tag(&mut tags, "Multisig");
    }
    if has("getMinDelay")
        || has("schedule")
        || (has("release") && (has("beneficiary") || text.contains("releaseTime")))
    {
        push_tag(&mut tags, "Timelock");
    }
    if text.contains("nonReentrant") || text.contains("ReentrancyGuard") {
        push_tag(&mut tags, "ReentrancyGuard");
    }
    if has("cap") {
        push_tag(&mut tags, "Capped");
    }

    tags
}

fn push_tag(tags: &mut Vec<String>, t: &str) {
    if !tags.iter().any(|x| x == t) {
        tags.push(t.to_string());
    }
}

/// One constructor parameter: its Solidity type (first token, data-location
/// keywords stripped) and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CtorParam {
    pub ty: String,
    pub name: String,
}

/// Parse the parameter list of the first `constructor(...)` in `source`.
/// Returns an empty vec when there is no constructor or it takes no
/// parameters. Inheritance/modifier args after the parameter list (e.g.
/// `ERC20("n","s")`) are ignored — only the constructor's own parameters.
pub fn extract_constructor(source: &str) -> Vec<CtorParam> {
    let s = strip_comments(source);
    let Some(idx) = s.find("constructor") else {
        return Vec::new();
    };
    let after_kw = idx + "constructor".len();
    let Some(open_rel) = s[after_kw..].find('(') else {
        return Vec::new();
    };
    let open = after_kw + open_rel;
    let Some(close) = match_paren(&s, open) else {
        return Vec::new();
    };
    s[open + 1..close]
        .split(',')
        .filter_map(|p| {
            let toks: Vec<&str> = p.split_whitespace().collect();
            if toks.len() < 2 {
                return None;
            }
            Some(CtorParam {
                ty: toks[0].to_string(),
                name: toks[toks.len() - 1].to_string(),
            })
        })
        .collect()
}

/// Synthesize a plausible default constructor argument for a parameter type,
/// for the auto-generated intent-path scaffold. Returns `None` for types we
/// cannot fabricate generically (interface/contract handles, arrays, structs)
/// — the caller then falls back to a no-arg deployment.
pub fn synthesize_ctor_arg(ty: &str) -> Option<String> {
    if ty.ends_with("[]") {
        return None;
    }
    match ty {
        "address" => Some("address(this)".to_string()),
        "bool" => Some("false".to_string()),
        "string" => Some("\"X\"".to_string()),
        "bytes" => Some("\"\"".to_string()),
        "bytes32" => Some("bytes32(0)".to_string()),
        _ if ty.starts_with("uint") => {
            let bits: u32 = ty
                .strip_prefix("uint")
                .filter(|b| !b.is_empty())
                .and_then(|b| b.parse().ok())
                .unwrap_or(256);
            // Stay within each width while giving token amounts real headroom.
            let v = if bits <= 8 {
                "1"
            } else if bits <= 16 {
                "100"
            } else if bits < 96 {
                "1000"
            } else {
                "1000000 ether"
            };
            Some(v.to_string())
        }
        _ if ty.starts_with("int") => Some("1".to_string()),
        // Interface/contract types (IERC20, etc.), structs, enums: no generic value.
        _ => None,
    }
}

/// Render a constructor invocation argument list (without parens) for `params`,
/// or `None` if any parameter type cannot be synthesized.
pub fn synthesize_ctor_args(params: &[CtorParam]) -> Option<String> {
    if params.is_empty() {
        return Some(String::new());
    }
    let mut out = Vec::with_capacity(params.len());
    for p in params {
        out.push(synthesize_ctor_arg(&p.ty)?);
    }
    Some(out.join(", "))
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

    #[test]
    fn detect_interfaces_erc20_not_erc721() {
        let src = r#"
            contract Token {
                function totalSupply() external view returns (uint256) {}
                function balanceOf(address a) external view returns (uint256) {}
                function transfer(address to, uint256 v) external returns (bool) {}
                function transferFrom(address f, address t, uint256 v) external returns (bool) {}
                function approve(address s, uint256 v) external returns (bool) {}
                function allowance(address o, address s) external view returns (uint256) {}
            }
        "#;
        let tags = detect_interfaces(src);
        assert!(tags.contains(&"ERC20".to_string()), "expected ERC20 in {tags:?}");
        assert!(
            !tags.contains(&"ERC721".to_string()),
            "ERC20 must not be tagged ERC721: {tags:?}"
        );
    }

    #[test]
    fn detect_interfaces_erc721_not_erc20() {
        // The exact stragglers' contract shape. Must NOT carry the ERC20 tag,
        // or ERC20 templates leak back into ERC721 retrieval (the A1 root cause).
        let src = include_str!("../../../examples/erc721/src/ERC721.sol");
        let tags = detect_interfaces(src);
        assert!(tags.contains(&"ERC721".to_string()), "expected ERC721 in {tags:?}");
        assert!(
            !tags.contains(&"ERC20".to_string()),
            "ERC721 must not be tagged ERC20: {tags:?}"
        );
    }

    #[test]
    fn detect_interfaces_vault_is_erc4626_and_erc20() {
        let src = r#"
            contract Vault {
                function asset() external view returns (address) {}
                function totalAssets() external view returns (uint256) {}
                function convertToShares(uint256 a) external view returns (uint256) {}
                function deposit(uint256 a, address r) external returns (uint256) {}
                function redeem(uint256 s, address r, address o) external returns (uint256) {}
                function transfer(address to, uint256 v) external returns (bool) {}
                function transferFrom(address f, address t, uint256 v) external returns (bool) {}
                function allowance(address o, address s) external view returns (uint256) {}
            }
        "#;
        let tags = detect_interfaces(src);
        assert!(tags.contains(&"ERC4626".to_string()), "expected ERC4626 in {tags:?}");
        assert!(
            tags.contains(&"ERC20".to_string()),
            "vault shares are ERC20: {tags:?}"
        );
    }

    #[test]
    fn detect_interfaces_additive_mixins() {
        let src = r#"
            contract PausableOwnableToken {
                function owner() external view returns (address) {}
                function transferOwnership(address n) external {}
                function paused() external view returns (bool) {}
                function transfer(address to, uint256 v) external returns (bool) {}
                function transferFrom(address f, address t, uint256 v) external returns (bool) {}
                function allowance(address o, address s) external view returns (uint256) {}
            }
        "#;
        let tags = detect_interfaces(src);
        assert!(tags.contains(&"ERC20".to_string()));
        assert!(tags.contains(&"Ownable".to_string()));
        assert!(tags.contains(&"Pausable".to_string()));
    }

    #[test]
    fn extract_constructor_parses_params_ignoring_inheritance_args() {
        let src = r#"
            contract T is ERC20Capped {
                constructor(uint256 cap_, uint256 supply, address owner)
                    ERC20("Cap", "CAP") ERC20Capped(cap_) { _mint(owner, supply); }
            }
        "#;
        let p = extract_constructor(src);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].ty, "uint256");
        assert_eq!(p[2].ty, "address");
        assert_eq!(p[2].name, "owner");
    }

    #[test]
    fn extract_constructor_empty_for_no_arg_ctor() {
        assert!(extract_constructor("contract C { constructor() ERC20(\"a\",\"b\") {} }").is_empty());
    }

    #[test]
    fn synthesize_ctor_args_handles_common_types() {
        let params = extract_constructor(
            "contract C { constructor(uint256 a, address b, bool c, uint8 d) {} }",
        );
        assert_eq!(
            synthesize_ctor_args(&params).as_deref(),
            Some("1000000 ether, address(this), false, 1")
        );
    }

    #[test]
    fn synthesize_ctor_args_none_for_interface_param() {
        // ERC20Wrapper(IERC20 underlying) — cannot fabricate a token handle.
        let params = extract_constructor("contract C { constructor(IERC20 u) ERC20Wrapper(u) {} }");
        assert_eq!(params.len(), 1);
        assert!(synthesize_ctor_args(&params).is_none());
    }

    #[test]
    fn detect_interfaces_unrecognized_is_empty() {
        // A contract with no recognizable surface → empty → caller won't filter.
        let src = r#"
            contract Blob {
                function doThing(uint256 x) external returns (uint256) {}
            }
        "#;
        assert!(detect_interfaces(src).is_empty());
    }
}
