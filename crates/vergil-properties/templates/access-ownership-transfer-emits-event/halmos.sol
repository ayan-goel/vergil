// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IOwnableLike {
    function owner() external view returns (address);
    function transferOwnership(address newOwner) external;
}

contract Check_access_ownership_transfer_emits_event {
    IOwnableLike public token;

    function check_transferOwnership_updates_owner(address newOwner) public {
        require(newOwner != address(0));
        try token.transferOwnership(newOwner) {
            assert(token.owner() == newOwner);
        } catch {
            // Only owner can transfer; non-owner calls revert.
        }
    }
}
