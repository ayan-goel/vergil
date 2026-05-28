// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {TransparentUpgradeableProxy} from "@openzeppelin/contracts/proxy/transparent/TransparentUpgradeableProxy.sol";

contract LogicV1 {
    uint256 public x;
    function setX(uint256 v) external { x = v; }
}

/// A transparent upgradeable proxy (auto-deploys its own ProxyAdmin).
contract Contract is TransparentUpgradeableProxy {
    constructor(address logic, address admin)
        TransparentUpgradeableProxy(logic, admin, "") {}
}
