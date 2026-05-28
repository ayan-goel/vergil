// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {ERC4626} from "@openzeppelin/contracts/token/ERC20/extensions/ERC4626.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";

contract Asset is ERC20 {
    constructor() ERC20("Asset", "AST") { _mint(msg.sender, 1e24); }
}

/// A deposit-capped ERC4626 vault.
contract Contract is ERC4626 {
    uint256 public constant CAP = 1_000_000e18;
    constructor(IERC20 a) ERC20("CapVault", "CV") ERC4626(a) {}

    function maxDeposit(address) public view override returns (uint256) {
        uint256 assets = totalAssets();
        return assets >= CAP ? 0 : CAP - assets;
    }
}
