// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ISlotReadWriteLike {
    function writeSlot(bytes32 slot, uint256 v) external;
    function readSlot(bytes32 slot) external view returns (uint256);
}

contract Check_proxy_storage_slot_stability {
    ISlotReadWriteLike internal target;

    function check_slot_write_read(bytes32 slot, uint256 v) external {
        target.writeSlot(slot, v);
        assert(target.readSlot(slot) == v);
    }
}
