// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

contract Contract {
    function ethSigned(bytes32 h) external pure returns (bytes32) {
        return MessageHashUtils.toEthSignedMessageHash(h);
    }
}
