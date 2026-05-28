// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal tl;
    constructor() {
        address[] memory ps = new address[](1);
        ps[0] = address(this);
        address[] memory es = new address[](1);
        es[0] = address(this);
        tl = new Contract(2 days, ps, es, address(this));
    }

    /// The configured minimum delay is reported.
    function check_min_delay_set() external view {
        assert(tl.getMinDelay() == 2 days);
    }

    /// An operation that was never scheduled is not pending.
    function check_unscheduled_not_pending(bytes32 id) external view {
        assert(!tl.isOperationPending(id));
    }
}
