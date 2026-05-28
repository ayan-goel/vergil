// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    function check_decimals_is_six() external view { assert(token.decimals() == 6); }

    function check_transfer_preserves_supply(address to, uint256 amount) external {
        require(amount <= token.balanceOf(address(this)));
        uint256 s = token.totalSupply();
        try token.transfer(to, amount) {} catch {}
        assert(token.totalSupply() == s);
    }
}
