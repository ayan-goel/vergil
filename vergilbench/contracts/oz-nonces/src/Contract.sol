// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Nonces} from "@openzeppelin/contracts/utils/Nonces.sol";

contract Contract is Nonces {
    function use(address owner) external returns (uint256) { return _useNonce(owner); }
}
