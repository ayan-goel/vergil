// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc4626Like {
    function maxDeposit(address) external view returns (uint256);
    function deposit(uint256, address) external returns (uint256);
    function maxRedeem(address) external view returns (uint256);
    function redeem(uint256, address, address) external returns (uint256);
}

contract Check_erc4626_max_functions_bounded {
    IErc4626Like public vault;

    function check_deposit_within_maxDeposit_does_not_revert(address receiver) public {
        require(receiver != address(0));
        uint256 cap = vault.maxDeposit(receiver);
        require(cap > 0 && cap < type(uint128).max);
        try vault.deposit(cap, receiver) returns (uint256) {} catch {
            assert(false);
        }
    }

    function check_redeem_within_maxRedeem_does_not_revert() public {
        uint256 cap = vault.maxRedeem(address(this));
        require(cap > 0 && cap < type(uint128).max);
        try vault.redeem(cap, address(this), address(this)) returns (uint256) {} catch {
            assert(false);
        }
    }
}
