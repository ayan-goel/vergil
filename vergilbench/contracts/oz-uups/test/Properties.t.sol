// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal impl;
    constructor() { impl = new Contract(); }

    /// proxiableUUID is the canonical EIP-1967 implementation slot.
    function check_proxiable_uuid_is_erc1967_slot() external view {
        assert(
            impl.proxiableUUID()
                == 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc
        );
    }
}
