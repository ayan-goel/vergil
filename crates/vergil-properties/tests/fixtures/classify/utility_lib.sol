// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

// Phase 3 fixture — pure utility library. No token surface, no
// vault / lending / AMM signatures, no role modifiers. The classifier
// must return 0 matches.
library Base64 {
    bytes internal constant TABLE =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    function encode(bytes memory data) internal pure returns (string memory) {
        if (data.length == 0) return "";
        // Stub — real impl would do the encoding.
        return string(TABLE);
    }
}
