// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal c;
    address internal forwarder = address(0xF0);
    constructor() { c = new Contract(forwarder); }

    /// Only the configured forwarder is trusted.
    function check_only_configured_forwarder_trusted(address other) external view {
        require(other != forwarder);
        assert(c.isTrustedForwarder(forwarder));
        assert(!c.isTrustedForwarder(other));
    }
}
