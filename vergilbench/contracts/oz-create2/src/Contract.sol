// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Create2} from "@openzeppelin/contracts/utils/Create2.sol";

contract Contract {
    function computed(bytes32 salt, bytes32 codeHash) external view returns (address) {
        return Create2.computeAddress(salt, codeHash);
    }
    function computedFor(bytes32 salt, bytes32 codeHash, address deployer) external pure returns (address) {
        return Create2.computeAddress(salt, codeHash, deployer);
    }
}
