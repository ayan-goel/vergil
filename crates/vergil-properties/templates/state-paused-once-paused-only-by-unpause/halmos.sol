// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IPausableLike {
    function paused() external view returns (bool);
    function unpause() external;
}

contract Check_state_paused_only_by_unpause {
    IPausableLike public target;

    function check_only_unpause_clears_paused() public {
        require(target.paused());
        try target.unpause() {
            // unpause is allowed to fail (auth, etc.); we only care that
            // after a successful unpause, paused() is false.
            assert(!target.paused());
        } catch {}
    }
}
