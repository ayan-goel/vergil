// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract MockToken {
    mapping(address => uint256) public balanceOf;
    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }
}

/// Beanstalk Farms (Apr 2022) — $182M drained via governance flash loan.
///
/// Reproduction note: Beanstalk's `emergencyCommit` allowed bypassing
/// the standard governance delay if the proposer held a
/// supermajority of stalk (governance token). The check was a spot
/// `balanceOf` read — flash-loanable. The attacker borrowed enough
/// BEAN3CRV-f LP from Aave to mint qualifying stalk, called
/// emergencyCommit on a malicious proposal that swept BEAN reserves,
/// then repaid the flash loan within one transaction.
contract BeanstalkGovernance {
    MockToken public immutable token;
    uint256 public constant QUORUM = 1_000_000;
    uint256 public actionsExecuted;

    constructor() {
        token = new MockToken();
    }

    /// Bug: spot balance read. A flash-loaned balance suffices to
    /// pass this gate.
    function privileged() external {
        require(token.balanceOf(msg.sender) >= QUORUM, "Beanstalk: not enough stalk");
        actionsExecuted++;
    }
}
