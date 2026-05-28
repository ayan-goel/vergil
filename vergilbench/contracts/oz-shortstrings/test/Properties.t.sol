// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// A short string survives a toShortString/toString round-trip.
    function check_round_trip_preserves() external view {
        assert(keccak256(bytes(c.roundTrip("vergil"))) == keccak256(bytes("vergil")));
    }
}
