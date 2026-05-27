// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IOwnableLike {
    function owner() external view returns (address);
    function renounceOwnership() external;
}

contract Check_access_renounce_zeros_owner {
    IOwnableLike public token;

    function check_renounceOwnership_zeros_owner() public {
        try token.renounceOwnership() {
            assert(token.owner() == address(0));
        } catch {
            // Non-owner renounce reverts; only assert on success.
        }
    }
}
