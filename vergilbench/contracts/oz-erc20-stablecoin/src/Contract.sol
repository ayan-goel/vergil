// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Pausable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Pausable.sol";
import {ERC20Permit} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Permit.sol";
import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";

/// A realistic stablecoin shape: pausable, EIP-2612 permit, role-gated mint.
contract Contract is ERC20Pausable, ERC20Permit, AccessControl {
    bytes32 public constant MINTER = keccak256("MINTER");
    bytes32 public constant PAUSER = keccak256("PAUSER");

    constructor() ERC20("Stable", "USD") ERC20Permit("Stable") {
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(MINTER, msg.sender);
        _grantRole(PAUSER, msg.sender);
    }

    function mint(address to, uint256 amount) external onlyRole(MINTER) { _mint(to, amount); }
    function pause() external onlyRole(PAUSER) { _pause(); }
    function unpause() external onlyRole(PAUSER) { _unpause(); }

    function _update(address from, address to, uint256 value)
        internal override(ERC20, ERC20Pausable)
    { super._update(from, to, value); }
}
