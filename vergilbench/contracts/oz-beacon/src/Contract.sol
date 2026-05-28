// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {UpgradeableBeacon} from "@openzeppelin/contracts/proxy/beacon/UpgradeableBeacon.sol";

contract LogicV1 {
    function answer() external pure returns (uint256) { return 42; }
}

/// An upgradeable beacon many BeaconProxies can point at.
contract Contract is UpgradeableBeacon {
    constructor(address impl, address owner) UpgradeableBeacon(impl, owner) {}
}
