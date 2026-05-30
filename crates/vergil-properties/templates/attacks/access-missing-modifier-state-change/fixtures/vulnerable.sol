// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable contract: `setProtected` lacks an access modifier, so any
/// caller can mutate the protected slot. The Parity Multisig (Nov 2017)
/// is the canonical real-world instance — `initWallet` was callable by
/// anyone, allowing ownership seizure then a kill() that froze ~$280M.
contract Target {
    address public owner;
    uint256 public protectedValue;

    constructor() {
        owner = msg.sender;
    }

    // BUG: missing modifier. Any caller can change protectedValue.
    function setProtected(uint256 newValue) external {
        protectedValue = newValue;
    }
}
