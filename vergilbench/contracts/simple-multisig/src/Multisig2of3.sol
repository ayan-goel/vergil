// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// 2-of-3 multisig confirmation tracker. Phase 4 Slice A8 bench corpus:
/// probes the canonical M-of-N authorization pattern under symbolic
/// execution. Three immutable signers, two confirmations required.
contract Multisig2of3 {
    address public immutable signer0;
    address public immutable signer1;
    address public immutable signer2;
    mapping(uint256 => mapping(address => bool)) public confirmed;
    mapping(uint256 => uint256) public confirmationCount;
    mapping(uint256 => bool) public executed;

    constructor(address a, address b, address c) {
        signer0 = a;
        signer1 = b;
        signer2 = c;
    }

    function isSigner(address who) public view returns (bool) {
        return who == signer0 || who == signer1 || who == signer2;
    }

    function confirm(uint256 txId) external {
        require(isSigner(msg.sender), "signer");
        require(!confirmed[txId][msg.sender], "double");
        confirmed[txId][msg.sender] = true;
        unchecked {
            confirmationCount[txId] += 1;
        }
    }

    function execute(uint256 txId) external returns (bool) {
        require(!executed[txId], "executed");
        require(confirmationCount[txId] >= 2, "threshold");
        executed[txId] = true;
        return true;
    }
}
