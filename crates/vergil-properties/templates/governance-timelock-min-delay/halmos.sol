// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITimelockControllerLike {
    function getMinDelay() external view returns (uint256);
    function isOperationPending(bytes32 id) external view returns (bool);
}

contract Check_governance_timelock_min_delay {
    ITimelockControllerLike internal timelock;
    uint256 internal expectedMinDelay;

    function check_min_delay_and_unscheduled(bytes32 id) external view {
        assert(timelock.getMinDelay() == expectedMinDelay);
        assert(!timelock.isOperationPending(id));
    }
}
