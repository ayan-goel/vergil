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

    /// With a decimals offset, the vault decimals exceed the asset decimals.
    function check_decimals_offset_applied() external view {
        assert(vault.decimals() == asset.decimals() + 3);
    }

    /// An empty vault still reports its asset.
    function check_asset_is_set() external view {
        assert(vault.asset() == address(asset));
    }
}
