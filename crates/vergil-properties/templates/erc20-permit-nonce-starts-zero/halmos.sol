// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC2612Like {
    function nonces(address owner) external view returns (uint256);
}

contract Check_erc20_permit_nonce_starts_zero {
    IERC2612Like internal token;

    function check_initial_nonce_zero(address owner) external view {
        assert(token.nonces(owner) == 0);
    }
}
