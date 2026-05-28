// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A single multicall batching setX and setY applies both writes.
    function check_multicall_applies_all(uint256 a, uint256 b) external {
        bytes[] memory calls = new bytes[](2);
        calls[0] = abi.encodeCall(Contract.setX, (a));
        calls[1] = abi.encodeCall(Contract.setY, (b));
        c.multicall(calls);
        assert(c.x() == a);
        assert(c.y() == b);
    }
}
