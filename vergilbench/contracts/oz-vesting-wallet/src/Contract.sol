// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {VestingWallet} from "@openzeppelin/contracts/finance/VestingWallet.sol";

contract Contract is VestingWallet {
    constructor(address beneficiary, uint64 start, uint64 duration)
        VestingWallet(beneficiary, start, duration) {}
}
