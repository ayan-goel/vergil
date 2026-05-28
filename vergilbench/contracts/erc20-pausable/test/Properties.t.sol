// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {PausableToken} from "../src/PausableToken.sol";

contract Properties {
    PausableToken internal token;

    constructor() {
        token = new PausableToken(1_000_000 ether);
    }

    /// When the pause flag is on, transfer reverts.
    function check_paused_blocks_transfer(address to, uint256 amount) external {
        token.pause();
        try token.transfer(to, amount) returns (bool) {
            // Transfer should NOT succeed while paused.
            assert(false);
        } catch {
            // Reverted as expected.
        }
    }

    /// pause() then unpause() leaves paused == false.
    function check_unpause_clears_flag() external {
        token.pause();
        token.unpause();
        assert(!token.paused());
    }

    /// Transfer preserves totalSupply when unpaused.
    function check_transfer_preserves_totalSupply(address to, uint256 amount) external {
        uint256 t0 = token.totalSupply();
        try token.transfer(to, amount) {
            assert(token.totalSupply() == t0);
        } catch {}
    }
}
