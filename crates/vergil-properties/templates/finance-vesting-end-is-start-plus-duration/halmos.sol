// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IVestingLike {
    function start() external view returns (uint256);
    function duration() external view returns (uint256);
    function end() external view returns (uint256);
}

contract Check_finance_vesting_end_is_start_plus_duration {
    IVestingLike internal vesting;

    function check_end_is_start_plus_duration() external view {
        assert(vesting.end() == vesting.start() + vesting.duration());
    }
}
