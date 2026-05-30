// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// Clean: identical to vulnerable.sol but with the minimum defense —
/// `require(sharesOut > 0)`. Any deposit that would round to zero
/// shares reverts. The full OpenZeppelin virtual-shares mitigation is
/// a stronger generalization (V2 enhancement).
contract Target {
    uint256 public totalShares;
    uint256 public totalAssets;
    mapping(address => uint256) public sharesOf;

    function deposit(uint256 assets) external returns (uint256 sharesOut) {
        if (totalShares == 0) {
            sharesOut = assets;
        } else {
            sharesOut = (assets * totalShares) / totalAssets;
        }
        require(sharesOut > 0, "Target: zero-share deposit");
        sharesOf[msg.sender] += sharesOut;
        totalShares += sharesOut;
        totalAssets += assets;
    }

    function donate(uint256 assets) external {
        totalAssets += assets;
    }
}
