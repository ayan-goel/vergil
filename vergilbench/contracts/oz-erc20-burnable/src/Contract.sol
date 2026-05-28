// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Burnable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";

/// Real-world OZ ERC20Burnable. burn()/burnFrom() debit supply.
contract Contract is ERC20Burnable {
    constructor(uint256 s) ERC20("Burnable", "BRN") {
        _mint(msg.sender, s);
    }
}
