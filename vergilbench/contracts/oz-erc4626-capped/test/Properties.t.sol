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

    /// maxDeposit never exceeds the configured cap.
    function check_max_deposit_within_cap(address who) external view {
        assert(vault.maxDeposit(who) <= vault.CAP());
    }
}
