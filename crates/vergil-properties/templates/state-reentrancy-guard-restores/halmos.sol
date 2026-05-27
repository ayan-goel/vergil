// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IGuardedLike {
    function reentrancyStatus() external view returns (uint256);
    function NOT_ENTERED() external view returns (uint256);
    function callNonReentrant() external;
}

contract Check_state_reentrancy_guard_restores {
    IGuardedLike public token;

    function check_guard_restores_after_call() public {
        uint256 unlocked = token.NOT_ENTERED();
        // Precondition: contract starts unlocked.
        require(token.reentrancyStatus() == unlocked);
        try token.callNonReentrant() {
            assert(token.reentrancyStatus() == unlocked);
        } catch {
            // Even on revert, the status slot must reset to unlocked
            // (Solidity reverts undo state, so this is the natural
            // invariant under the guard pattern).
            assert(token.reentrancyStatus() == unlocked);
        }
    }
}
