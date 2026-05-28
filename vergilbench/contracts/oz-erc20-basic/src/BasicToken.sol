// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// Minimal concrete ERC-20 over the OpenZeppelin v5.1.0 implementation — the
/// real-world contract under test. Mints a fixed supply to the deployer; all
/// transfer/approve/allowance behavior is OZ's `ERC20`.
contract BasicToken is ERC20 {
    constructor(uint256 initialSupply) ERC20("BasicToken", "BTK") {
        _mint(msg.sender, initialSupply);
    }
}
