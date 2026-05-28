// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// A transfer that would push the recipient above MAX_WALLET reverts.
    function check_over_max_reverts(address to, uint256 amount) external {
        require(to != address(this) && to != address(0));
        require(token.balanceOf(to) == 0);
        require(amount > token.MAX_WALLET() && amount <= token.balanceOf(address(this)));
        try token.transfer(to, amount) { assert(false); } catch {}
    }
}
