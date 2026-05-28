// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface INoncesLike {
    function nonces(address owner) external view returns (uint256);
    function use(address owner) external returns (uint256);
}

contract Check_util_nonces_increment {
    INoncesLike internal target;

    function check_use_increments_nonce(address owner) external {
        uint256 prev = target.nonces(owner);
        target.use(owner);
        assert(target.nonces(owner) == prev + 1);
    }
}
