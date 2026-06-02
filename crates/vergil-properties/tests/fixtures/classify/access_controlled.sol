// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// No specific primitive signal — just role-based access. The
// AccessControlledGeneric catch-all fires at 0.65.
contract AdminCounter {
    address public owner;
    uint256 public count;

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    constructor() { owner = msg.sender; }

    function bump() external onlyOwner { count += 1; }
}
