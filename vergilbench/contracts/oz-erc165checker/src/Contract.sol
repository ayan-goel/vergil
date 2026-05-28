// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC165} from "@openzeppelin/contracts/utils/introspection/ERC165.sol";
import {ERC165Checker} from "@openzeppelin/contracts/utils/introspection/ERC165Checker.sol";

contract Target is ERC165 {}

contract Contract {
    function isERC165(address a) external view returns (bool) {
        return ERC165Checker.supportsERC165(a);
    }
}
