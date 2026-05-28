// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// ERC-20 with a pauser-gated transfer. Phase 4 Slice A8 bench corpus.
/// The pause/unpause is exposed to anyone for the symbolic test — a real
/// contract would role-gate. Property tests pin: when paused, transfer
/// reverts.
contract PausableToken {
    mapping(address => uint256) public balanceOf;
    uint256 public totalSupply;
    bool public paused;

    constructor(uint256 initialSupply) {
        balanceOf[msg.sender] = initialSupply;
        totalSupply = initialSupply;
    }

    function pause() external {
        paused = true;
    }

    function unpause() external {
        paused = false;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        require(!paused, "paused");
        require(balanceOf[msg.sender] >= amount, "balance");
        unchecked {
            balanceOf[msg.sender] -= amount;
            balanceOf[to] += amount;
        }
        return true;
    }
}
