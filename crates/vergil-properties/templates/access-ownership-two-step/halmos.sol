// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IOwnable2StepLike {
    function owner() external view returns (address);
    function pendingOwner() external view returns (address);
    function transferOwnership(address) external;
}

contract Check_access_ownership_two_step {
    IOwnable2StepLike public target;

    function check_transferOwnership_does_not_change_active_owner(address newOwner) public {
        address before = target.owner();
        try target.transferOwnership(newOwner) {
            assert(target.owner() == before);
            assert(target.pendingOwner() == newOwner);
        } catch {}
    }
}
