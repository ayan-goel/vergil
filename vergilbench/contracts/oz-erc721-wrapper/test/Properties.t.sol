// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract, Underlying} from "../src/Contract.sol";
import {IERC721} from "@openzeppelin/contracts/token/ERC721/IERC721.sol";

contract Properties {
    Contract internal wrapper;
    Underlying internal underlying;
    constructor() {
        underlying = new Underlying();
        wrapper = new Contract(IERC721(address(underlying)));
    }

    /// The wrapper reports the underlying collection it wraps.
    function check_underlying_is_set() external view {
        assert(address(wrapper.underlying()) == address(underlying));
    }
}
