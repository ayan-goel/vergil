// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

contract Counter {
    mapping(address => uint256) public count;

    function inc(address who, uint128 by) external {
        count[who] += by;
    }

    function dec(address who, uint128 by) external {
        require(count[who] >= by, "underflow");
        count[who] -= by;
    }
}
