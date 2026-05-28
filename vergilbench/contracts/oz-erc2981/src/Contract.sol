// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC2981} from "@openzeppelin/contracts/token/common/ERC2981.sol";

contract Contract is ERC2981 {
    constructor(address receiver) { _setDefaultRoyalty(receiver, 1000); } // 10%
}
