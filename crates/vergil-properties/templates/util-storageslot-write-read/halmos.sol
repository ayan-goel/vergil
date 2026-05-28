// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IStorageSlotLike {
    function write(bytes32 slot, uint256 v) external;
    function read(bytes32 slot) external view returns (uint256);
}

contract Check_util_storageslot_write_read {
    IStorageSlotLike internal target;

    function check_slot_write_read(bytes32 slot, uint256 v) external {
        target.write(slot, v);
        assert(target.read(slot) == v);
    }
}
