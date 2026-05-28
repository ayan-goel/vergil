// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Contract} from "../src/Contract.sol";

contract Properties {
    Contract internal token;
    constructor() { token = new Contract(1_000_000e18); }

    /// A fresh account's permit nonce starts at zero.
    function check_initial_nonce_zero(address owner) external view {
        assert(token.nonces(owner) == 0);
    }

    /// An expired permit (deadline in the past) reverts.
    function check_expired_permit_reverts(address owner, address spender, uint256 value) external {
        uint256 pastDeadline = 0;
        try token.permit(owner, spender, value, pastDeadline, 27, bytes32(0), bytes32(0)) {
            assert(false);
        } catch {}
    }
}
