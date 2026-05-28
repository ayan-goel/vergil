// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITimelockLike {
    function releaseAt() external view returns (uint256);
    function release() external returns (uint256);
}

contract Check_time_lock_pre_deadline_reverts {
    ITimelockLike public lock;

    /// release() reverts when block.timestamp < releaseAt.
    function check_release_before_deadline_reverts() external {
        require(block.timestamp < lock.releaseAt());
        try lock.release() returns (uint256) {
            assert(false);
        } catch {}
    }
}
