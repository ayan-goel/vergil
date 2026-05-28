// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Votes} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Votes.sol";
import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";

/// Real-world OZ governance token (ERC20 + checkpointed votes).
contract Contract is ERC20Votes {
    constructor() ERC20("Gov", "GOV") EIP712("Gov", "1") {}
    function mint(address to, uint256 amount) external { _mint(to, amount); }
}
