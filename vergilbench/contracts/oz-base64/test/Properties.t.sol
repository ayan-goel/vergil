// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// Encoding empty input yields an empty string.
    function check_empty_encodes_empty() external view {
        assert(bytes(c.enc("")).length == 0);
    }

    /// Base64 encodes every 3 input bytes into 4 output chars (1 byte -> 4 padded chars).
    function check_one_byte_encodes_to_four() external view {
        assert(bytes(c.enc(hex"00")).length == 4);
    }
}
