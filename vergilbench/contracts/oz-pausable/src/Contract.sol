// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";

contract Contract is Pausable {
    uint256 public counter;
    function pause() external { _pause(); }
    function unpause() external { _unpause(); }
    function bump() external whenNotPaused { counter += 1; }
}
