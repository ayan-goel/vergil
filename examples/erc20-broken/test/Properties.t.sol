// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {TokenBroken} from "../src/TokenBroken.sol";

/// @notice Halmos symbolic properties. The contract is buggy
///         (transferFrom skips the allowance check); these properties
///         should produce counterexamples.
contract Properties {
    TokenBroken internal token;

    constructor() {
        token = new TokenBroken(1_000_000 ether);
    }

    /// transferFrom from an owner who has not approved the caller must revert.
    /// The broken contract allows the transfer, so this assertion is reachable
    /// and Halmos surfaces a counterexample.
    function check_transferFrom_blocks_unauthorized(address to, uint256 amount) external {
        require(to != address(0));
        require(to != address(this));
        require(amount > 0);

        // Property contract is the owner (constructor minted to it) and the caller.
        // We never call approve, so allowance[this][this] is zero.
        require(token.allowance(address(this), address(this)) == 0);
        require(token.balanceOf(address(this)) >= amount);

        token.transferFrom(address(this), to, amount);

        // Correct ERC-20 reverts above; reaching here means the bug let it through.
        assert(false);
    }
}
