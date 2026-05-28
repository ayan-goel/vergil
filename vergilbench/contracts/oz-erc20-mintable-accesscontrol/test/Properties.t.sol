// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// The minter role increases supply by exactly the minted amount.
    function check_role_mint_increases_supply(address to, uint256 amount) external {
        require(to != address(0));
        uint256 s = token.totalSupply();
        token.mint(to, amount);
        assert(token.totalSupply() == s + amount);
    }

    /// Minter admin is the default admin role.
    function check_minter_admin_is_default() external view {
        assert(token.getRoleAdmin(token.MINTER()) == token.DEFAULT_ADMIN_ROLE());
    }
}
