// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Checkpoints} from "@openzeppelin/contracts/utils/structs/Checkpoints.sol";

/// Checkpointed value history (the structure ERC20Votes/Governor build on).
contract Contract {
    using Checkpoints for Checkpoints.Trace208;
    Checkpoints.Trace208 private _trace;

    function push(uint48 key, uint208 value) external { _trace.push(key, value); }
    function latest() external view returns (uint208) { return _trace.latest(); }
    function length() external view returns (uint256) { return _trace.length(); }
}
