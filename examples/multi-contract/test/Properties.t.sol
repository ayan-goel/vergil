// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Token} from "../src/Token.sol";
import {Vault} from "../src/Vault.sol";

/// Phase 4 Slice A4 — cross-contract property tests.
///
/// The vault starts with the entire token supply pre-deposited (1:1)
/// so the initial state already satisfies the share-asset invariant;
/// every property check tests that the invariant is preserved through
/// vault operations on symbolic inputs.
contract Properties {
    Token internal token;
    Vault internal vault;

    constructor() {
        // Mint the entire supply to this test contract, then seed the
        // vault with the full balance + credit shares 1:1. Sets up the
        // share-asset invariant at construction.
        token = new Token(1_000_000 ether, address(this));
        vault = new Vault(token);
        token.transfer(address(vault), 1_000_000 ether);
        vault.depositFor(address(this), 1_000_000 ether);
    }

    /// Core cross-contract invariant: the vault's recorded total shares
    /// equal the underlying token balance the vault holds. This is the
    /// canonical "shares match assets" property; any divergence means
    /// either the vault's share accounting drifted from the token
    /// balance OR a deposit/redeem operation lost atomicity.
    function check_vault_shares_match_token_balance() external view {
        assert(vault.totalShares() == token.balanceOf(address(vault)));
    }

    /// Redeem preserves the cross-contract invariant. After a partial
    /// redeem, the vault's recorded shares still equal its token
    /// balance.
    function check_redeem_preserves_share_asset_match(uint256 amount) external {
        // Constrain to within the test contract's available shares so
        // the redeem doesn't revert for a reason unrelated to the
        // invariant (we're testing the invariant, not deposit edge cases).
        require(amount <= vault.shares(address(this)));
        require(amount <= 1_000 ether); // keep symbolic search tractable
        vault.redeem(amount);
        assert(vault.totalShares() == token.balanceOf(address(vault)));
    }

    /// The total shares the vault has issued equals the sum of shares
    /// across all holders. Single-holder case here (the constructor
    /// only credits this contract), so this collapses to:
    /// totalShares == shares(this) + shares(some-other-address-zero).
    function check_vault_total_shares_account_for_holders(address other) external view {
        require(other != address(this));
        // We've only credited shares to address(this) at construction.
        // Any other address has zero shares; totalShares == shares(this).
        assert(vault.shares(other) == 0);
        assert(vault.totalShares() == vault.shares(address(this)));
    }
}
