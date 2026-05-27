// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc4626Like {
    function previewDeposit(uint256) external view returns (uint256);
    function deposit(uint256, address) external returns (uint256);
    function previewRedeem(uint256) external view returns (uint256);
    function redeem(uint256, address, address) external returns (uint256);
}

contract Check_erc4626_preview_matches_actual {
    IErc4626Like public vault;

    function check_previewDeposit_matches_deposit(uint256 assets) public {
        uint256 predicted;
        try vault.previewDeposit(assets) returns (uint256 p) {
            predicted = p;
        } catch {
            return;
        }
        try vault.deposit(assets, address(this)) returns (uint256 actual) {
            assert(actual >= predicted);
        } catch {}
    }

    function check_previewRedeem_matches_redeem(uint256 shares) public {
        uint256 predicted;
        try vault.previewRedeem(shares) returns (uint256 p) {
            predicted = p;
        } catch {
            return;
        }
        try vault.redeem(shares, address(this), address(this)) returns (uint256 actual) {
            assert(actual >= predicted);
        } catch {}
    }
}
