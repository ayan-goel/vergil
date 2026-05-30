// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: pull-payment style — `distribute` credits per-recipient
/// ledgers without external calls. Each recipient independently
/// triggers their own `withdraw`.
contract Target {
    mapping(address => uint256) public credited;

    function distribute(address a, address b, uint256 amount) external {
        credited[a] += amount;
        credited[b] += amount;
    }

    function withdraw() external {
        uint256 amount = credited[msg.sender];
        credited[msg.sender] = 0;
        (bool ok, ) = msg.sender.call{value: amount}("");
        require(ok, "Target: withdraw failed");
    }
}
