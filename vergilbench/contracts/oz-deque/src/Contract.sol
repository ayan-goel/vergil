// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {DoubleEndedQueue} from "@openzeppelin/contracts/utils/structs/DoubleEndedQueue.sol";

contract Contract {
    using DoubleEndedQueue for DoubleEndedQueue.Bytes32Deque;
    DoubleEndedQueue.Bytes32Deque private _q;

    function pushBack(bytes32 v) external { _q.pushBack(v); }
    function popFront() external returns (bytes32) { return _q.popFront(); }
    function front() external view returns (bytes32) { return _q.front(); }
    function length() external view returns (uint256) { return _q.length(); }
    function empty() external view returns (bool) { return _q.empty(); }
}
