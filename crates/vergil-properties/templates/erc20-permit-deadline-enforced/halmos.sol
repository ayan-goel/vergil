// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC2612PermitLike {
    function permit(
        address owner, address spender, uint256 value,
        uint256 deadline, uint8 v, bytes32 r, bytes32 s
    ) external;
}

contract Check_erc20_permit_deadline_enforced {
    IERC2612PermitLike internal token;

    function check_expired_permit_reverts(address owner, address spender, uint256 value) external {
        try token.permit(owner, spender, value, 0, 27, bytes32(0), bytes32(0)) {
            assert(false);
        } catch {}
    }
}
