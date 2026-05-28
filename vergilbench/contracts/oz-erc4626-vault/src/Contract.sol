// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {ERC4626} from "@openzeppelin/contracts/token/ERC20/extensions/ERC4626.sol";

/// Asset the vault holds.
contract Asset is ERC20 {
    constructor() ERC20("Asset", "AST") { _mint(msg.sender, 1e24); }
}

/// Real-world OZ ERC4626 tokenized vault.
contract Contract is ERC4626 {
    constructor(IERC20 asset_) ERC20("Vault", "VLT") ERC4626(asset_) {}
}
