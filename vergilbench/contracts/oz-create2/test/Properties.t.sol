// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    constructor() { c = new Contract(); }

    /// CREATE2 address derivation is a deterministic function of (salt, codeHash, deployer).
    function check_create2_is_deterministic(bytes32 salt, bytes32 codeHash, address deployer) external view {
        assert(c.computedFor(salt, codeHash, deployer) == c.computedFor(salt, codeHash, deployer));
    }

    /// Different deployers (vs this contract) generally derive different addresses for the same salt/codeHash.
    function check_create2_binds_deployer(bytes32 salt, bytes32 codeHash, address deployer) external view {
        require(deployer != address(c));
        // The derivation includes the deployer, so a non-matching deployer must change the result.
        assert(c.computedFor(salt, codeHash, deployer) != c.computed(salt, codeHash));
    }
}
