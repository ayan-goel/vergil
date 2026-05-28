// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITraceLike {
    function push(uint48 key, uint208 value) external;
    function latest() external view returns (uint208);
    function length() external view returns (uint256);
}

contract Check_struct_checkpoints_push_latest {
    ITraceLike internal trace;

    function check_push_sets_latest(uint48 key, uint208 value) external {
        trace.push(key, value);
        assert(trace.latest() == value);
        assert(trace.length() == 1);
    }
}
