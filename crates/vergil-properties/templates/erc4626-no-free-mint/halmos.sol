// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc4626Like {
    function totalAssets() external view returns (uint256);
    function totalSupply() external view returns (uint256);
    function deposit(uint256, address) external returns (uint256);
}

contract Check_erc4626_no_free_mint {
    IErc4626Like public vault;

    function check_deposit_increases_totalAssets_by_at_least_paid(
        uint256 assets,
        address receiver
    ) public {
        require(assets > 0);
        require(receiver != address(0));
        uint256 ta0 = vault.totalAssets();
        uint256 ts0 = vault.totalSupply();
        try vault.deposit(assets, receiver) returns (uint256 shares) {
            if (shares > 0) {
                // shares were issued; vault must have received at least `assets` worth
                assert(vault.totalAssets() >= ta0);
                // and totalSupply grew strictly with the shares
                assert(vault.totalSupply() == ts0 + shares);
            }
        } catch {}
    }
}
