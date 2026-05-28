// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Timelock} from "../src/Timelock.sol";

contract Properties {
    Timelock internal lock;

    constructor() {
        // Release time 1_000_000 (some block.timestamp); seeded balance.
        lock = new Timelock(address(this), 1_000_000, 100 ether);
    }

    /// release() reverts when block.timestamp is below releaseAt.
    function check_release_before_deadline_reverts() external {
        // Halmos models block.timestamp symbolically; we constrain via
        // the require inside release().
        try lock.release() returns (uint256) {
            // If this succeeded, block.timestamp must be >= releaseAt.
            assert(block.timestamp >= lock.releaseAt());
        } catch {}
    }

    /// release() can only succeed once.
    function check_release_idempotent() external {
        try lock.release() returns (uint256) {
            try lock.release() returns (uint256) {
                // Second call must have failed.
                assert(false);
            } catch {}
        } catch {}
    }

    /// After a successful release, balance reads as zero.
    function check_release_drains_balance() external {
        try lock.release() returns (uint256) {
            assert(lock.balance() == 0);
        } catch {}
    }
}
