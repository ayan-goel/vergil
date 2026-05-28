// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";

contract Impl {
    function ping() external pure returns (bool) { return true; }
}

/// An EIP-1167 minimal-proxy clone factory.
contract Contract {
    address public immutable implementation;
    constructor() { implementation = address(new Impl()); }

    function cloneDet(bytes32 salt) external returns (address) {
        return Clones.cloneDeterministic(implementation, salt);
    }
    function predict(bytes32 salt) external view returns (address) {
        return Clones.predictDeterministicAddress(implementation, salt, address(this));
    }
}
