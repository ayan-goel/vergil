// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {AccessControl} from "@openzeppelin/contracts/access/AccessControl.sol";

contract Contract is ERC20, AccessControl {
    bytes32 public constant MINTER = keccak256("MINTER");
    constructor() ERC20("RoleMint", "RM") {
        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(MINTER, msg.sender);
    }
    function mint(address to, uint256 amount) external onlyRole(MINTER) { _mint(to, amount); }
}
