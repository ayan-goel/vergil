// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC4626} from "../src/ERC4626.sol";

// Hand-written Halmos check_ functions for examples/vault-4626.
// These mirror four of the five kill-criterion ERC-4626 ground-truth
// properties; the fifth (deposit_then_redeem_does_not_inflate) is
// documented in the README as a known stragger.
contract Properties {
    ERC4626 internal token;

    constructor() {
        // Seed the vault with non-zero assets so conversions are
        // well-defined. The seed goes to address(this) so the contract
        // can later deposit.
        token = new ERC4626(1_000_000 ether, address(this));
    }

    /// More input assets convert to at least as many shares — never fewer.
    function check_convertToShares_is_monotone(uint256 a1, uint256 a2) external view {
        require(a1 <= a2);
        // Bound to avoid overflow during multiplication when totalAssets >= 1.
        require(a2 <= type(uint128).max);
        assert(token.convertToShares(a1) <= token.convertToShares(a2));
    }

    /// More input shares convert to at least as many assets — never fewer.
    function check_convertToAssets_is_monotone(uint256 s1, uint256 s2) external view {
        require(s1 <= s2);
        require(s2 <= type(uint128).max);
        assert(token.convertToAssets(s1) <= token.convertToAssets(s2));
    }

    /// Round-trip: assets → shares → assets cannot produce more than
    /// the input assets.
    function check_roundtrip_assets_does_not_inflate(uint256 assets) external view {
        require(assets <= type(uint128).max);
        uint256 shares = token.convertToShares(assets);
        uint256 backToAssets = token.convertToAssets(shares);
        assert(backToAssets <= assets);
    }

    /// If deposit succeeds, totalAssets increases by exactly the assets
    /// the depositor provided.
    function check_deposit_increases_totalAssets_by_at_least_paid(
        uint256 assets,
        address receiver
    ) external {
        require(receiver != address(0));
        require(assets <= type(uint128).max);
        uint256 before = token.totalAssets();
        try token.deposit(assets, receiver) returns (uint256) {
            assert(token.totalAssets() >= before + assets);
        } catch {
            // Deposit may revert (insufficient asset balance, zero receiver);
            // only assert when it returned.
        }
    }
}
