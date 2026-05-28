// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC1363} from "@openzeppelin/contracts/token/ERC20/extensions/ERC1363.sol";
import {IERC1363} from "@openzeppelin/contracts/interfaces/IERC1363.sol";

contract Contract is ERC1363 {
    constructor() ERC20("Payable", "PAY") { _mint(msg.sender, 1e21); }
}
