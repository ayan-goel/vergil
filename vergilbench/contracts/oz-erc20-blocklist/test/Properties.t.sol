// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// A blocked sender cannot transfer.
    function check_blocked_sender_cannot_transfer(address to, uint256 amount) external {
        require(to != address(this) && to != address(0));
        token.setBlocked(address(this), true);
        require(amount <= token.balanceOf(address(this)));
        try token.transfer(to, amount) { assert(false); } catch {}
    }

    /// A transfer to a blocked recipient reverts.
    function check_blocked_recipient_cannot_receive(address to, uint256 amount) external {
        require(to != address(this) && to != address(0));
        token.setBlocked(to, true);
        require(amount <= token.balanceOf(address(this)));
        try token.transfer(to, amount) { assert(false); } catch {}
    }
}
