// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IMulticallTargetLike {
    function setX(uint256 v) external;
    function setY(uint256 v) external;
    function multicall(bytes[] calldata data) external returns (bytes[] memory);
    function x() external view returns (uint256);
    function y() external view returns (uint256);
}

contract Check_util_multicall_applies_all {
    IMulticallTargetLike internal target;

    function check_multicall_applies_all(uint256 a, uint256 b) external {
        bytes[] memory calls = new bytes[](2);
        calls[0] = abi.encodeWithSelector(target.setX.selector, a);
        calls[1] = abi.encodeWithSelector(target.setY.selector, b);
        target.multicall(calls);
        assert(target.x() == a);
        assert(target.y() == b);
    }
}
