// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract, BoxV1} from "../src/Contract.sol";

contract Properties {
    BoxV1 internal impl;
    Contract internal proxy;
    constructor() {
        impl = new BoxV1();
        proxy = new Contract(address(impl));
    }

    /// State written through the proxy persists in the proxy's storage.
    function check_proxy_delegates_state(uint256 v) external {
        BoxV1(address(proxy)).setValue(v);
        assert(BoxV1(address(proxy)).value() == v);
    }
}
