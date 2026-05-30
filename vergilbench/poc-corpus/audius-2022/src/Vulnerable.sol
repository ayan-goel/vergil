// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Audius governance takeover (Jul 2022).
///
/// Reproduction note: Audius's upgradeable Governance contract had an
/// `initialize()` that set `guardianAddress = msg.sender` without an
/// `initializer`-style guard. The attacker called it on the proxy,
/// became guardian, queued + executed a proposal draining AUDIO. We
/// reduce to the canonical "initialize lets msg.sender become owner,
/// no guard against re-init" shape; the template's encoding (attacker
/// contract attempts a second initialize after the legitimate one)
/// detects the missing guard.
contract AudiusGovernance {
    address public owner;
    uint256 public quorumThreshold;
    bool public emergencyMode;

    /// Bug: no `initializer` modifier, no `require(owner == address(0))` —
    /// any caller can become `owner`, including after a legitimate init.
    function initialize() external {
        owner = msg.sender;
        quorumThreshold = 100;
    }

    function setQuorumThreshold(uint256 t) external {
        require(msg.sender == owner, "AudiusGovernance: not owner");
        quorumThreshold = t;
    }
}
