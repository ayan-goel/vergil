// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A signature of invalid length yields a zero address and a non-zero error code.
    function check_bad_length_is_error(bytes32 hash) external view {
        bytes memory sig = hex"1234"; // 2 bytes, not 64/65
        (address rec, uint8 err) = c.tryRecoverLen(hash, sig);
        assert(rec == address(0));
        assert(err != 0);
    }
}
