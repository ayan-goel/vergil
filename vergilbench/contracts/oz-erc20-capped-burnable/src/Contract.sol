// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Capped} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Capped.sol";
import {ERC20Burnable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";

contract Contract is ERC20Capped, ERC20Burnable {
    constructor() ERC20("CapBurn", "CB") ERC20Capped(1_000_000e18) {
        _mint(msg.sender, 400_000e18);
    }
    function mint(address to, uint256 amount) external { _mint(to, amount); }

    function _update(address from, address to, uint256 value)
        internal override(ERC20, ERC20Capped)
    { super._update(from, to, value); }
}
