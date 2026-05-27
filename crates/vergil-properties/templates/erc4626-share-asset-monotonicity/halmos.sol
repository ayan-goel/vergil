// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc4626Like {
    function convertToShares(uint256) external view returns (uint256);
    function convertToAssets(uint256) external view returns (uint256);
}

contract Check_erc4626_share_asset_monotonicity {
    IErc4626Like public vault;

    function check_convertToShares_is_monotone(uint256 a, uint256 b) public view {
        require(a <= b);
        try vault.convertToShares(a) returns (uint256 sa) {
            try vault.convertToShares(b) returns (uint256 sb) {
                assert(sa <= sb);
            } catch {}
        } catch {}
    }

    function check_convertToAssets_is_monotone(uint256 a, uint256 b) public view {
        require(a <= b);
        try vault.convertToAssets(a) returns (uint256 aa) {
            try vault.convertToAssets(b) returns (uint256 ab) {
                assert(aa <= ab);
            } catch {}
        } catch {}
    }

    function check_roundtrip_assets_does_not_inflate(uint256 assets) public view {
        try vault.convertToShares(assets) returns (uint256 shares) {
            try vault.convertToAssets(shares) returns (uint256 back) {
                assert(back <= assets);
            } catch {}
        } catch {}
    }
}
