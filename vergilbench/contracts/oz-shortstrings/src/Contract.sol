// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ShortStrings, ShortString} from "@openzeppelin/contracts/utils/ShortStrings.sol";

contract Contract {
    function roundTrip(string calldata s) external pure returns (string memory) {
        return ShortStrings.toString(ShortStrings.toShortString(s));
    }
}
