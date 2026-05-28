// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {OwnableCounter} from "../src/OwnableCounter.sol";

contract Properties {
    OwnableCounter internal counter;

    constructor() {
        counter = new OwnableCounter(address(this));
    }

    /// The Properties contract is the owner; increment succeeds and
    /// advances count by 1.
    function check_owner_can_increment() external {
        uint256 before = counter.count();
        counter.increment();
        assert(counter.count() == before + 1);
    }

    /// transferOwnership to zero reverts.
    function check_transferOwnership_to_zero_reverts() external {
        try counter.transferOwnership(address(0)) {
            assert(false);
        } catch {}
    }

    /// After a successful transferOwnership, owner reads as the new address.
    function check_transferOwnership_updates_owner(address newOwner) external {
        require(newOwner != address(0));
        counter.transferOwnership(newOwner);
        assert(counter.owner() == newOwner);
    }
}
