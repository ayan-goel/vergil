// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// transferOwnership only stages a pending owner; the owner is unchanged.
    function check_transfer_only_stages_pending(address n) external {
        require(n != address(0));
        c.transferOwnership(n);
        assert(c.pendingOwner() == n);
        assert(c.owner() == address(this));
    }
}
