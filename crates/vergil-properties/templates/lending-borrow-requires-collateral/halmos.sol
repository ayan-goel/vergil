// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ILendingLike {
    function collateral(address) external view returns (uint256);
    function debt(address) external view returns (uint256);
    function LTV_BPS() external view returns (uint256);
    function borrow(uint256 amount) external;
}

contract Check_lending_borrow_requires_collateral {
    ILendingLike public token;

    function check_borrow_requires_collateral(uint256 amount) public {
        require(amount > 0 && amount <= type(uint64).max);
        uint256 c = token.collateral(address(this));
        uint256 d = token.debt(address(this));
        uint256 capacity = (c * token.LTV_BPS()) / 100;
        uint256 newDebt = d + amount;
        if (capacity < newDebt) {
            try token.borrow(amount) {
                assert(false);
            } catch {
                // Expected.
            }
        }
    }
}
