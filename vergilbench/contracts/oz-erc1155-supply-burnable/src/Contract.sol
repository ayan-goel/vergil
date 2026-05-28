// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC1155} from "@openzeppelin/contracts/token/ERC1155/ERC1155.sol";
import {ERC1155Supply} from "@openzeppelin/contracts/token/ERC1155/extensions/ERC1155Supply.sol";
import {ERC1155Burnable} from "@openzeppelin/contracts/token/ERC1155/extensions/ERC1155Burnable.sol";

contract Contract is ERC1155Supply, ERC1155Burnable {
    constructor() ERC1155("ipfs://{id}") {}
    function mint(address to, uint256 id, uint256 amount) external { _mint(to, id, amount, ""); }

    function _update(address from, address to, uint256[] memory ids, uint256[] memory values)
        internal override(ERC1155, ERC1155Supply)
    { super._update(from, to, ids, values); }
}
