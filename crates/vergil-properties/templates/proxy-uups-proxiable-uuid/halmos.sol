// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IUUPSLike {
    function proxiableUUID() external view returns (bytes32);
}

contract Check_proxy_uups_proxiable_uuid {
    IUUPSLike internal impl;

    function check_proxiable_uuid_is_erc1967_slot() external view {
        assert(
            impl.proxiableUUID()
                == 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc
        );
    }
}
