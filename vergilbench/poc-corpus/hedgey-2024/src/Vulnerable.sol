// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Hedgey Finance (Apr 2024) — $44.7M token-claim exploit.
///
/// Reproduction note: Hedgey's `cancelClaim` marked the plan inactive
/// but left the allowance live. Combined with a missing caller-check
/// on cancelClaim, anyone could cancel any plan AND the recipient
/// retained transferFrom rights to drain. Catalog template
/// `logic-approval-not-revoked-after-cancel` catches the
/// stale-allowance bug.
contract HedgeyClaim {
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
        require(msg.sender == owner, "HedgeyClaim: not owner");
        id = nextId++;
        plans[id] = Plan(recipient, amount, true);
    }

    /// Bug 1: no caller-validation (anyone can cancel any plan).
    /// Bug 2: marks inactive but doesn't zero `allowance` — recipient
    /// retains transferFrom rights against the protocol's account.
    function cancelPlan(uint256 id) external {
        plans[id].active = false;
    }
}
