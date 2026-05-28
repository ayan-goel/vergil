// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {VestingWalletCliff} from "@openzeppelin/contracts/finance/VestingWalletCliff.sol";
import {VestingWallet} from "@openzeppelin/contracts/finance/VestingWallet.sol";

contract Contract is VestingWalletCliff {
    constructor(address beneficiary, uint64 start, uint64 duration, uint64 cliff)
        VestingWallet(beneficiary, start, duration) VestingWalletCliff(cliff) {}
}
