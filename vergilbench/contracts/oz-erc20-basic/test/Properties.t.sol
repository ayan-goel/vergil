// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {BasicToken} from "../src/BasicToken.sol";

/// Properties over OpenZeppelin's ERC-20. The Properties contract is the
/// deployer and therefore holds the full initial supply.
contract Properties {
    BasicToken internal token;
    address internal alice = address(0xA11CE);

    constructor() {
        token = new BasicToken(1_000_000e18);
    }

    /// transfer moves exactly `amount` from caller to a distinct recipient.
    function check_transfer_conserves_balances(uint256 amount) external {
        require(alice != address(this));
        require(amount <= token.balanceOf(address(this)));
        uint256 beforeSelf = token.balanceOf(address(this));
        uint256 beforeAlice = token.balanceOf(alice);
        token.transfer(alice, amount);
        assert(token.balanceOf(address(this)) == beforeSelf - amount);
        assert(token.balanceOf(alice) == beforeAlice + amount);
    }

    /// totalSupply is invariant under transfer.
    function check_transfer_preserves_supply(address to, uint256 amount) external {
        require(amount <= token.balanceOf(address(this)));
        uint256 supply = token.totalSupply();
        try token.transfer(to, amount) {} catch {}
        assert(token.totalSupply() == supply);
    }

    /// approve sets allowance to exactly the requested value.
    function check_approve_sets_allowance(address spender, uint256 amount) external {
        token.approve(spender, amount);
        assert(token.allowance(address(this), spender) == amount);
    }
}
