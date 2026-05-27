// SPDX-License-Identifier: Apache-2.0
// Vergil reference lending primitive: simplified Compound-style market.
// One asset, no interest-rate model, no price oracle (price = 1:1 between
// collateral and debt). Inlines uint256 accounting; no external ERC-20.
//
// Operations:
//   deposit — add collateral
//   borrow — take debt, requires collateralValue * LTV / 100 >= debt + amount
//   repay — pay down debt
//   liquidate — close an undercollateralized account, transferring its
//     collateral to the liquidator
//
// Invariants we verify:
//   1. After any single op, totalCollateral >= totalDebt (solvency).
//   2. borrow() reverts unless collateral * LTV / 100 >= new debt.
//   3. liquidate() reverts when the account is not undercollateralized.

pragma solidity ^0.8.20;

contract Lending {
    /// Loan-to-value ratio, percent. Borrow capacity = collateral * LTV / 100.
    uint256 public constant LTV_BPS = 75; // 75%

    mapping(address => uint256) public collateral;
    mapping(address => uint256) public debt;

    uint256 public totalCollateral;
    uint256 public totalDebt;

    function deposit(uint256 amount) external {
        require(amount > 0, "Lending: zero deposit");
        unchecked {
            collateral[msg.sender] += amount;
            totalCollateral += amount;
        }
    }

    function borrow(uint256 amount) external {
        require(amount > 0, "Lending: zero borrow");
        uint256 newDebt = debt[msg.sender] + amount;
        uint256 capacity = (collateral[msg.sender] * LTV_BPS) / 100;
        require(capacity >= newDebt, "Lending: insufficient collateral");
        debt[msg.sender] = newDebt;
        unchecked {
            totalDebt += amount;
        }
    }

    function repay(uint256 amount) external {
        require(amount > 0, "Lending: zero repay");
        uint256 d = debt[msg.sender];
        require(d >= amount, "Lending: over-repay");
        unchecked {
            debt[msg.sender] = d - amount;
            totalDebt -= amount;
        }
    }

    function liquidate(address account) external {
        require(account != address(0), "Lending: zero account");
        uint256 c = collateral[account];
        uint256 d = debt[account];
        uint256 capacity = (c * LTV_BPS) / 100;
        // Only undercollateralized accounts can be liquidated.
        require(capacity < d, "Lending: account is solvent");
        // Liquidator receives collateral, debt is wiped.
        unchecked {
            collateral[account] = 0;
            debt[account] = 0;
            totalCollateral -= c;
            totalDebt -= d;
            collateral[msg.sender] += c;
            totalCollateral += c;
        }
    }
}
