// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IWrapperLike {
    function underlying() external view returns (address);
}

contract Check_erc20_wrapper_underlying_set {
    IWrapperLike internal token;
    address internal expectedUnderlying;

    function check_underlying_is_set() external view {
        assert(token.underlying() == expectedUnderlying);
    }
}
