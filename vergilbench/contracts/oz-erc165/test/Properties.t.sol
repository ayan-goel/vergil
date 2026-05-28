// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// Supports the ERC-165 interface id and rejects the invalid 0xffffffff id.
    function check_erc165_self_and_invalid() external view {
        assert(c.supportsInterface(0x01ffc9a7));
        assert(!c.supportsInterface(0xffffffff));
    }
}
