// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// Both feature invariants hold together: supply <= cap.
    function check_supply_at_most_cap() external view {
        assert(token.totalSupply() <= token.cap());
    }

    /// While paused, transfers revert.
    function check_paused_blocks_transfer(address to, uint256 amount) external {
        token.pause();
        try token.transfer(to, amount) { assert(false); } catch {}
    }
}
