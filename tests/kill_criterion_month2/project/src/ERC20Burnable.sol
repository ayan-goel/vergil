// SPDX-License-Identifier: Apache-2.0
// Vergil reference ERC-20 with a burn extension. Semantics match
// OpenZeppelin's `ERC20Burnable.sol`: any holder can burn their own
// balance, or burn on behalf of another address by spending allowance.

pragma solidity ^0.8.20;

import {ERC20} from "./ERC20.sol";

contract ERC20Burnable is ERC20 {
    constructor(string memory name_, string memory symbol_, uint256 initialSupply, address mintTo)
        ERC20(name_, symbol_, initialSupply, mintTo)
    {}

    function burn(uint256 amount) external {
        _burn(msg.sender, amount);
    }

    function burnFrom(address from, uint256 amount) external {
        _spendAllowance(from, msg.sender, amount);
        _burn(from, amount);
    }
}
