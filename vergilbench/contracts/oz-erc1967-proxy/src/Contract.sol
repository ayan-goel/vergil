// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC1967Proxy} from "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

contract BoxV1 {
    uint256 public value;
    function setValue(uint256 v) external { value = v; }
}

/// A minimal EIP-1967 proxy pointing at a logic contract.
contract Contract is ERC1967Proxy {
    constructor(address impl) ERC1967Proxy(impl, "") {}
}
