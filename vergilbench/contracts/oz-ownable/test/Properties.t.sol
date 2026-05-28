// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// The deployer is the initial owner.
    function check_deployer_is_owner() external view {
        assert(c.owner() == address(this));
    }

    /// transferOwnership moves ownership to a non-zero new owner.
    function check_transfer_sets_owner(address n) external {
        require(n != address(0));
        c.transferOwnership(n);
        assert(c.owner() == n);
    }

    /// renounceOwnership zeroes the owner.
    function check_renounce_zeroes_owner() external {
        c.renounceOwnership();
        assert(c.owner() == address(0));
    }
}
