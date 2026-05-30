// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: implementation contract behind a UUPS proxy whose
/// constructor does NOT disable the initializer. After deployment,
/// anyone can call `initialize` directly on the implementation
/// (bypassing the proxy) and seize ownership. The Wormhole advisory
/// (Feb 2022, $10M white-hat bounty) is the canonical instance — the
/// broader uninitialized-proxy class drove the 2025 automated ERC1967
/// upgrade campaign.
contract Target {
    address public owner;
    bool private initialized;

    function initialize() external {
        require(!initialized, "Target: already initialized");
        initialized = true;
        owner = msg.sender;
    }
}
