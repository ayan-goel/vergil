// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: `onlyOwner` modifier gates the destroy callsite. See
/// vulnerable.sol for the SELFDESTRUCT encoding note.
contract Target {
    address public owner;
    mapping(address => bool) public claimedRecipient;

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "Target: not owner");
        _;
    }

    function destroy(address payable beneficiary) external onlyOwner {
        claimedRecipient[beneficiary] = true;
    }
}
