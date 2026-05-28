// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Base64} from "@openzeppelin/contracts/utils/Base64.sol";

contract Contract {
    function enc(bytes calldata data) external pure returns (string memory) { return Base64.encode(data); }
}
