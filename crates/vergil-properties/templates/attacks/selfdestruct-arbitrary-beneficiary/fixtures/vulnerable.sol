// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: anyone can call `destroy` and direct the funds to any
/// beneficiary. Parity Multisig (Nov 2017) is the canonical real-world
/// instance.
///
/// Encoding note: the literal SELFDESTRUCT opcode is unsupported by
/// Halmos (and post-EIP-6780 deprecated on mainnet). The fixture
/// structurally encodes the same auth bug — the verified property is
/// "non-owner cannot reach the destroy callsite" — and the per-
/// beneficiary withdraw bookkeeping is a stand-in for the value drain.
contract Target {
    address public owner;
    mapping(address => bool) public claimedRecipient;

    constructor() {
        owner = msg.sender;
    }

    function destroy(address payable beneficiary) external {
        // BUG: no caller auth check before the destructive callsite.
        claimedRecipient[beneficiary] = true;
    }
}
