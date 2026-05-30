// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: cancelPlan zeros the allowance and gates the caller. Resolves
/// both Hedgey-class flaws simultaneously (this template's allowance
/// invariant and the joint `input-missing-parameter-validation` check).
contract Target {
    struct Plan {
        address recipient;
        uint256 allowance;
        bool active;
    }

    address public owner;
    mapping(uint256 => Plan) public plans;
    uint256 public nextId;

    constructor() {
        owner = msg.sender;
    }

    function createPlan(address recipient, uint256 amount) external returns (uint256 id) {
        require(msg.sender == owner, "Target: not owner");
        id = nextId++;
        plans[id] = Plan(recipient, amount, true);
    }

    function cancelPlan(uint256 id) external {
        require(id < nextId, "Target: invalid id");
        require(msg.sender == owner, "Target: not owner");
        plans[id].active = false;
        plans[id].allowance = 0;
    }
}
