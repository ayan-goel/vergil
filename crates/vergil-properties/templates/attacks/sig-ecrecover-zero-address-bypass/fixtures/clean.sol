// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: explicit zero-rejection before the auth comparison.
contract Target {
    address public signer;
    uint256 public executions;

    function setSigner(address s) external {
        signer = s;
    }

    function execute(address recovered) external {
        require(recovered != address(0), "Target: zero recover");
        require(recovered == signer, "Target: bad sig");
        executions++;
    }
}
