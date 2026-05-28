// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IDequeLike {
    function pushBack(bytes32 v) external;
    function front() external view returns (bytes32);
    function length() external view returns (uint256);
    function empty() external view returns (bool);
}

contract Check_struct_deque_push_front {
    IDequeLike internal q;

    function check_push_front_roundtrip(bytes32 v) external {
        assert(q.empty());
        q.pushBack(v);
        assert(q.length() == 1);
        assert(q.front() == v);
    }
}
