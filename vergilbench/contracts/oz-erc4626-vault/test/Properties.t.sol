// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract, Asset} from "../src/Contract.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract Properties {
    Contract internal vault;
    Asset internal asset;
    constructor() {
        asset = new Asset();
        vault = new Contract(IERC20(address(asset)));
    }

    /// The vault reports the asset it was constructed with.
    function check_asset_is_set() external view {
        assert(vault.asset() == address(asset));
    }

    /// An empty vault holds zero assets.
    function check_total_assets_zero_initially() external view {
        assert(vault.totalAssets() == 0);
    }
}
