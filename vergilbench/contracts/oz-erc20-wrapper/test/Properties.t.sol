// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract, Underlying} from "../src/Contract.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract Properties {
    Contract internal token;
    Underlying internal underlying;
    constructor() {
        underlying = new Underlying();
        token = new Contract(IERC20(address(underlying)));
    }

    /// The wrapper reports the underlying it was constructed with.
    function check_underlying_is_set() external view {
        assert(address(token.underlying()) == address(underlying));
    }

    /// Before any deposit, the wrapper has zero supply.
    function check_initial_supply_zero() external view {
        assert(token.totalSupply() == 0);
    }
}
