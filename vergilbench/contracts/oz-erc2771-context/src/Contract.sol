// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC2771Context} from "@openzeppelin/contracts/metatx/ERC2771Context.sol";

/// A meta-transaction-aware contract trusting a single forwarder.
contract Contract is ERC2771Context {
    constructor(address forwarder) ERC2771Context(forwarder) {}
}
