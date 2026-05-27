// SPDX-License-Identifier: Apache-2.0
// Vergil reference ERC-20 with a pause switch. Semantics match
// OpenZeppelin's `Pausable` mixin: when paused, all balance-mutating
// public entry points revert. Read-only views are never paused.

pragma solidity ^0.8.20;

import {ERC20} from "./ERC20.sol";

contract ERC20Pausable is ERC20 {
    bool public paused;
    address public pauser;

    event Paused();
    event Unpaused();

    error EnforcedPause();
    error NotPauser();

    constructor(string memory name_, string memory symbol_, uint256 initialSupply, address mintTo)
        ERC20(name_, symbol_, initialSupply, mintTo)
    {
        pauser = msg.sender;
    }

    modifier whenNotPaused() {
        if (paused) revert EnforcedPause();
        _;
    }

    modifier onlyPauser() {
        if (msg.sender != pauser) revert NotPauser();
        _;
    }

    function pause() external onlyPauser {
        paused = true;
        emit Paused();
    }

    function unpause() external onlyPauser {
        paused = false;
        emit Unpaused();
    }

    // Override balance-mutating entry points. The internal helpers in
    // ERC20 are reused so the accounting logic stays in one place.

    function transfer(address to, uint256 amount)
        external
        override
        whenNotPaused
        returns (bool)
    {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount)
        external
        override
        whenNotPaused
        returns (bool)
    {
        _spendAllowance(from, msg.sender, amount);
        _transfer(from, to, amount);
        return true;
    }

    function approve(address spender, uint256 amount)
        external
        override
        whenNotPaused
        returns (bool)
    {
        _approve(msg.sender, spender, amount);
        return true;
    }
}
