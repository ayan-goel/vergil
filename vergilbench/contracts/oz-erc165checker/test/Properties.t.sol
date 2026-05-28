// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract, Target} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    Target internal t;
    constructor() { c = new Contract(); t = new Target(); }

    /// An ERC165 target is detected; the zero address is not.
    function check_detects_erc165() external view {
        assert(c.isERC165(address(t)));
        assert(!c.isERC165(address(0)));
    }
}
