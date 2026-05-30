// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Minimal fee-on-transfer ERC-20. Every transfer takes a 1-wei fee.
contract FoTToken {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transferFrom(address from, address to, uint256 amount)
        external
        returns (bool)
    {
        require(allowance[from][msg.sender] >= amount, "FoT: allow");
        require(balanceOf[from] >= amount, "FoT: bal");
        allowance[from][msg.sender] -= amount;
        balanceOf[from] -= amount;
        // FoT: 1-wei fee per transfer.
        uint256 credit = amount > 0 ? amount - 1 : 0;
        balanceOf[to] += credit;
        return true;
    }
}

/// Vulnerable: protocol's `deposit` credits the requested amount, NOT
/// the actually-received amount. FoT users get over-credited (positive
/// accounting drift the protocol cannot back).
contract Target {
    FoTToken public immutable token;
    mapping(address => uint256) public internalBalance;

    constructor() {
        token = new FoTToken();
    }

    function deposit(uint256 amount) external {
        token.transferFrom(msg.sender, address(this), amount);
        // BUG: credits requested amount; actual received is `amount - 1` (FoT fee).
        internalBalance[msg.sender] += amount;
    }
}
