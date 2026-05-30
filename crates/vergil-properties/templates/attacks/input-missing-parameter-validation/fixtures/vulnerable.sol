// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: cancelPlan marks the plan inactive but leaves the per-plan
/// allowance live. The recipient (or anyone who was previously granted
/// transferFrom rights) can still drain. Hedgey Finance (Apr 2024) is the
/// canonical instance — $42.6M (Arbitrum) + $2.1M (Ethereum), combined
/// with a missing caller-validation flaw covered by
/// `input-missing-parameter-validation`.
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

    // BUG: marks inactive but does not zero the allowance.
    // BUG (joint w/ input-missing-parameter-validation): no caller check;
    // anyone can cancel any plan.
    function cancelPlan(uint256 id) external {
        plans[id].active = false;
    }
}
