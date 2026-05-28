// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IFlaggedLike {
    function paused() external view returns (bool);
    function pause() external;
    function unpause() external;
}

contract Check_state_machine_flag_toggle {
    IFlaggedLike public token;

    /// pause() then unpause() always lands paused == false.
    function check_unpause_clears_flag() external {
        token.pause();
        token.unpause();
        assert(!token.paused());
    }
}
