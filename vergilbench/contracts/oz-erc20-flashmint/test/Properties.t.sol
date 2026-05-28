// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(1_000_000e18); }

    /// The default flash fee is zero.
    function check_default_flash_fee_zero(uint256 amount) external view {
        assert(token.flashFee(address(token), amount) == 0);
    }

    /// maxFlashLoan for an unrelated token address is zero.
    function check_max_flashloan_other_token_zero(address other) external view {
        require(other != address(token));
        assert(token.maxFlashLoan(other) == 0);
    }
}
