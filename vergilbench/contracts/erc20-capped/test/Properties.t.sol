// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {CappedToken} from "../src/CappedToken.sol";

contract Properties {
    CappedToken internal token;

    constructor() {
        token = new CappedToken(1_000_000 ether);
    }

    /// Mint always keeps totalSupply at or below the cap.
    function check_mint_respects_cap(address to, uint256 amount) external {
        try token.mint(to, amount) {
            assert(token.totalSupply() <= token.cap());
        } catch {}
    }

    /// Transfer preserves total supply.
    function check_transfer_preserves_totalSupply(address to, uint256 amount) external {
        uint256 t0 = token.totalSupply();
        try token.transfer(to, amount) {
            assert(token.totalSupply() == t0);
        } catch {}
    }

    /// totalSupply is monotone-non-decreasing relative to the cap.
    function check_totalSupply_below_cap_invariant() external view {
        assert(token.totalSupply() <= token.cap());
    }
}
