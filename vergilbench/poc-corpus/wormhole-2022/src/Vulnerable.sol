// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Wormhole-style UUPS uninitialized-implementation bug.
///
/// Reproduction note: the canonical UUPS proxy footgun — the
/// implementation contract's constructor does NOT call
/// `_disableInitializers()`. A freshly-deployed implementation is
/// thus directly initializable by anyone, even though the legitimate
/// state lives behind a proxy. The attacker becomes owner of the
/// implementation, then can selfdestruct it (pre-EIP-6780) or
/// otherwise corrupt the proxy's logic pointer.
contract WormholeImplementation {
    address public owner;
    bool public initialized;
    uint256 public chainId;
    bytes32 public guardianSetHash;

    /// Bug: no constructor-side `_disableInitializers()`, no
    /// `initializer` modifier — anyone can initialize the
    /// implementation contract.
    function initialize() external {
        require(!initialized, "Wormhole: already init");
        initialized = true;
        owner = msg.sender;
        chainId = 1;
    }

    function setGuardianSetHash(bytes32 h) external {
        require(msg.sender == owner, "Wormhole: not owner");
        guardianSetHash = h;
    }
}
