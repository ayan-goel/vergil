// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(1_000_000e18); }

    /// While paused, transfers revert.
    function check_paused_blocks_transfer(address to, uint256 amount) external {
        token.pause();
        require(amount <= token.balanceOf(address(this)));
        try token.transfer(to, amount) { assert(false); } catch {}
    }

    /// After unpause, the paused flag is clear.
    function check_unpause_clears_flag() external {
        token.pause();
        token.unpause();
        assert(!token.paused());
    }
}
