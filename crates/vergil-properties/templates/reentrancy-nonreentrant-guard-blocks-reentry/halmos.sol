// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IGuardedExternalLike {
    // Hypothesis: this function carries the nonReentrant modifier.
    function guardedAction() external;
    // Counter incremented inside guardedAction; reentry would double-increment.
    function counter() external view returns (uint256);
}

contract Check_reentrancy_nonreentrant_guard_blocks_reentry {
    IGuardedExternalLike public target;
    bool public reentering;

    receive() external payable {
        if (reentering) {
            // Attempt re-entry; if the guard works, this call reverts and we
            // observe the counter incremented by exactly one in the outer call.
            try target.guardedAction() {} catch {}
        }
    }

    function check_guarded_action_does_not_double_increment() public {
        uint256 c0 = target.counter();
        reentering = true;
        try target.guardedAction() {} catch {}
        reentering = false;
        uint256 c1 = target.counter();
        assert(c1 == c0 || c1 == c0 + 1);
    }
}
