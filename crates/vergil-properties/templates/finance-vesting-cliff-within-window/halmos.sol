// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IVestingCliffLike {
    function start() external view returns (uint256);
    function end() external view returns (uint256);
    function cliff() external view returns (uint256);
}

contract Check_finance_vesting_cliff_within_window {
    IVestingCliffLike internal vesting;

    function check_cliff_within_window() external view {
        assert(vesting.cliff() >= vesting.start());
        assert(vesting.cliff() <= vesting.end());
    }
}
