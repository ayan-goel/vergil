// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Permit} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Permit.sol";
import {ERC20Votes} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Votes.sol";
import {Nonces} from "@openzeppelin/contracts/utils/Nonces.sol";

/// The full OZ governance token: checkpointed votes + EIP-2612 permit.
contract Contract is ERC20Votes, ERC20Permit {
    constructor() ERC20("GovFull", "GVF") ERC20Permit("GovFull") {}
    function mint(address to, uint256 amount) external { _mint(to, amount); }

    function _update(address from, address to, uint256 value)
        internal override(ERC20, ERC20Votes)
    { super._update(from, to, value); }

    function nonces(address owner)
        public view override(ERC20Permit, Nonces) returns (uint256)
    { return super.nonces(owner); }
}
