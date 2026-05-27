// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IGuardedLike {
    function owner() external view returns (address);
    function setProtectedValue(uint256) external;
    function protectedValue() external view returns (uint256);
}

contract Check_access_onlyowner_storage_guard {
    IGuardedLike public target;

    function check_non_owner_cannot_mutate(uint256 newValue) public {
        require(target.owner() != address(this));
        uint256 v0 = target.protectedValue();
        try target.setProtectedValue(newValue) {
            // mutation must have failed to take effect
            assert(target.protectedValue() == v0);
        } catch {}
    }
}
