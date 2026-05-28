// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IBeaconLike {
    function implementation() external view returns (address);
    function owner() external view returns (address);
}

contract Check_proxy_beacon_implementation {
    IBeaconLike internal beacon;
    address internal expectedImpl;
    address internal expectedOwner;

    function check_beacon_implementation_and_owner() external view {
        assert(beacon.implementation() == expectedImpl);
        assert(beacon.owner() == expectedOwner);
    }
}
