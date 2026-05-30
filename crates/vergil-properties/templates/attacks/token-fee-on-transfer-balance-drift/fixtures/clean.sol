// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

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
        uint256 credit = amount > 0 ? amount - 1 : 0;
        balanceOf[to] += credit;
        return true;
    }
}

/// Clean: protocol uses the balance-before / balance-after pattern so
/// accounting tracks the actually-received amount.
contract Target {
    FoTToken public immutable token;
    mapping(address => uint256) public internalBalance;

    constructor() {
        token = new FoTToken();
    }

    function deposit(uint256 amount) external {
        uint256 before_ = token.balanceOf(address(this));
        token.transferFrom(msg.sender, address(this), amount);
        uint256 after_ = token.balanceOf(address(this));
        internalBalance[msg.sender] += (after_ - before_);
    }
}
