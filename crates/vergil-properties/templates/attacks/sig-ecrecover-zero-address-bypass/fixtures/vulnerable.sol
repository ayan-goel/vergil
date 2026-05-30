// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `signer` defaults to `address(0)` and the comparison
/// doesn't reject zero. Any forged input that yields a zero
/// "recovered" address passes the auth check.
contract Target {
    address public signer;
    uint256 public executions;

    function setSigner(address s) external {
        signer = s;
    }

    function execute(address recovered) external {
        // BUG: when signer == 0 (uninitialized), recovered == 0 succeeds.
        require(recovered == signer, "Target: bad sig");
        executions++;
    }
}
