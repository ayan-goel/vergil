// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    address internal treasury = address(0x7);
    constructor() { token = new Contract(treasury); }

    /// A transfer routes a 1% fee to the treasury and the remainder to the recipient.
    function check_fee_routes_to_treasury(address to, uint256 amount) external {
        require(to != address(this) && to != address(0) && to != treasury);
        require(amount <= token.balanceOf(address(this)) && amount <= 1e24);
        uint256 t0 = token.balanceOf(treasury);
        uint256 r0 = token.balanceOf(to);
        uint256 fee = amount * 100 / 10000;
        token.transfer(to, amount);
        assert(token.balanceOf(treasury) == t0 + fee);
        assert(token.balanceOf(to) == r0 + (amount - fee));
    }
}
