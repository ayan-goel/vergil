// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {ERC20Wrapper} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Wrapper.sol";

/// Underlying token the wrapper wraps 1:1.
contract Underlying is ERC20 {
    constructor() ERC20("Underlying", "UND") { _mint(msg.sender, 1e24); }
}

/// Real-world OZ ERC20Wrapper.
contract Contract is ERC20Wrapper {
    constructor(IERC20 u) ERC20("Wrapped", "WRP") ERC20Wrapper(u) {}
}
