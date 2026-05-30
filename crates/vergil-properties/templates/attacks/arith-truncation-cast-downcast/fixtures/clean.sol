// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract Target {
    function downcastTo128(uint256 x) external pure returns (uint128) {
        require(x <= type(uint128).max, "Target: overflow");
        return uint128(x);
    }
}
