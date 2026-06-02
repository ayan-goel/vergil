// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 5 S4 fixture — conservation positive case.
//
// `transfer` debits sender and credits recipient by the same amount —
// paired `-=`/`+=` ops on `balanceOf`. The miner emits one candidate
// per (mapping, function) pair.
contract ConservationTransfer {
    mapping(address => uint256) public balanceOf;

    function transfer(address to, uint256 amount) external {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
    }
}
