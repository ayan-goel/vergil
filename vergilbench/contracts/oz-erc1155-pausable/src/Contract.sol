// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC1155} from "@openzeppelin/contracts/token/ERC1155/ERC1155.sol";
import {ERC1155Pausable} from "@openzeppelin/contracts/token/ERC1155/extensions/ERC1155Pausable.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract Contract is ERC1155Pausable, Ownable {
    constructor() ERC1155("ipfs://{id}") Ownable(msg.sender) {}
    function mint(address to, uint256 id, uint256 amount) external { _mint(to, id, amount, ""); }
    function pause() external onlyOwner { _pause(); }
    function unpause() external onlyOwner { _unpause(); }
}
