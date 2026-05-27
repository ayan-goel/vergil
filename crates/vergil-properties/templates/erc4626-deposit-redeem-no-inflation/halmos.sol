// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc4626Like {
    function deposit(uint256 assets, address receiver) external returns (uint256 shares);
    function redeem(uint256 shares, address receiver, address owner) external returns (uint256 assets);
}

contract Check_erc4626_deposit_redeem_no_inflation {
    IErc4626Like public token;

    function check_deposit_then_redeem_does_not_inflate(uint256 assets) public {
        require(assets > 0 && assets <= type(uint128).max);
        try token.deposit(assets, address(this)) returns (uint256 shares) {
            require(shares > 0);
            try token.redeem(shares, address(this), address(this)) returns (uint256 back) {
                assert(back <= assets);
            } catch {
                // Redeem can revert on accounting edge; only assert on success.
            }
        } catch {
            // Deposit can revert on capacity; only assert on success.
        }
    }
}
