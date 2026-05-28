// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";
import {IERC1363} from "@openzeppelin/contracts/interfaces/IERC1363.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(); }

    /// The token advertises the ERC-1363 interface via ERC-165.
    function check_supports_erc1363() external view {
        assert(token.supportsInterface(type(IERC1363).interfaceId));
    }
}
