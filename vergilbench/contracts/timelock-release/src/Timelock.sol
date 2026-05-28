// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// Time-locked balance release. Phase 4 Slice A8 bench corpus: probes
/// block.timestamp gating under symbolic execution.
contract Timelock {
    address public immutable beneficiary;
    uint256 public immutable releaseAt;
    uint256 public balance;
    bool public released;

    constructor(address _beneficiary, uint256 _releaseAt, uint256 _balance) {
        beneficiary = _beneficiary;
        releaseAt = _releaseAt;
        balance = _balance;
    }

    function release() external returns (uint256) {
        require(block.timestamp >= releaseAt, "early");
        require(!released, "already");
        released = true;
        uint256 amount = balance;
        balance = 0;
        return amount;
    }
}
