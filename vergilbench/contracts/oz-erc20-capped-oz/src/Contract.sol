// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Capped} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Capped.sol";

/// Real-world OZ ERC20Capped. totalSupply can never exceed the cap.
contract Contract is ERC20Capped {
    constructor(uint256 cap_, uint256 s) ERC20("Capped", "CAP") ERC20Capped(cap_) {
        _mint(msg.sender, s);
    }
    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}
