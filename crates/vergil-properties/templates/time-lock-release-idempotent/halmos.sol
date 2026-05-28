// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITimelockLike {
    function released() external view returns (bool);
    function release() external returns (uint256);
}

contract Check_time_lock_release_idempotent {
    ITimelockLike public lock;

    /// Once released is true, a fresh release must revert.
    function check_release_idempotent() external {
        require(lock.released());
        try lock.release() returns (uint256) {
            assert(false);
        } catch {}
    }
}
