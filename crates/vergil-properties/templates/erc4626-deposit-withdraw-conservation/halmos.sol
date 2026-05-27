// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc4626Like {
    function deposit(uint256, address) external returns (uint256);
    function redeem(uint256, address, address) external returns (uint256);
}

contract Check_erc4626_deposit_withdraw_conservation {
    IErc4626Like public vault;

    function check_deposit_then_redeem_does_not_inflate(uint256 assets) public {
        require(assets > 0);
        uint256 shares;
        try vault.deposit(assets, address(this)) returns (uint256 s) {
            shares = s;
        } catch {
            return;
        }
        try vault.redeem(shares, address(this), address(this)) returns (uint256 returned) {
            assert(returned <= assets);
        } catch {}
    }
}
