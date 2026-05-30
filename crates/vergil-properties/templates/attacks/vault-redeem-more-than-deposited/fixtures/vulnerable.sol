// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Vulnerable: `redeem` uses an `unchecked` block to decrement the
/// caller's balance, so requesting more than owned wraps to a giant
/// uint256 instead of reverting. The withdrawal still credits the
/// caller's tracked assets.
contract Target {
    mapping(address => uint256) public sharesOf;
    uint256 public totalShares;
    mapping(address => uint256) public assetsClaimed;

    function deposit(uint256 amount) external returns (uint256) {
        sharesOf[msg.sender] += amount;
        totalShares += amount;
        return amount;
    }

    function redeem(uint256 shares) external returns (uint256) {
        unchecked {
            sharesOf[msg.sender] -= shares;
            totalShares -= shares;
        }
        assetsClaimed[msg.sender] += shares;
        return shares;
    }
}
