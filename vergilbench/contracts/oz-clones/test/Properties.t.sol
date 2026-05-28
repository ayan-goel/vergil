// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal factory;
    constructor() { factory = new Contract(); }

    /// The deterministic clone lands exactly at the predicted address.
    function check_clone_matches_prediction(bytes32 salt) external {
        address predicted = factory.predict(salt);
        address actual = factory.cloneDet(salt);
        assert(actual == predicted);
    }
}
