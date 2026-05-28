// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {UUPSUpgradeable} from "@openzeppelin/contracts/proxy/utils/UUPSUpgradeable.sol";

/// A minimal UUPS-upgradeable logic contract.
contract Contract is UUPSUpgradeable {
    function _authorizeUpgrade(address) internal override {}
    function version() external pure returns (uint256) { return 1; }
}
