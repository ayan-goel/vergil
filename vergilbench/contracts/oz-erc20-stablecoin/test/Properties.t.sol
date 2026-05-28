// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// The deployer holds the minter role.
    function check_deployer_is_minter() external view {
        assert(token.hasRole(token.MINTER(), address(this)));
    }

    /// While paused, transfers revert.
    function check_paused_blocks_transfer(address to, uint256 amount) external {
        token.mint(address(this), amount);
        token.pause();
        try token.transfer(to, amount) { assert(false); } catch {}
    }

    /// Role-gated mint increases supply by exactly the amount.
    function check_mint_increases_supply(address to, uint256 amount) external {
        require(to != address(0));
        uint256 before = token.totalSupply();
        token.mint(to, amount);
        assert(token.totalSupply() == before + amount);
    }
}
