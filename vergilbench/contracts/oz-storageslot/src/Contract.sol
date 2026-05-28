// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";

contract Contract {
    function write(bytes32 slot, uint256 v) external { StorageSlot.getUint256Slot(slot).value = v; }
    function read(bytes32 slot) external view returns (uint256) { return StorageSlot.getUint256Slot(slot).value; }
}
