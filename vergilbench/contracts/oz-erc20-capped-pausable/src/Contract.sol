// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Capped} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Capped.sol";
import {ERC20Pausable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Pausable.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract Contract is ERC20Capped, ERC20Pausable, Ownable {
    constructor() ERC20("CapPause", "CP") ERC20Capped(1_000_000e18) Ownable(msg.sender) {
        _mint(msg.sender, 100_000e18);
    }
    function mint(address to, uint256 amount) external onlyOwner { _mint(to, amount); }
    function pause() external onlyOwner { _pause(); }
    function unpause() external onlyOwner { _unpause(); }

    function _update(address from, address to, uint256 value)
        internal override(ERC20Capped, ERC20Pausable)
    { super._update(from, to, value); }
}
