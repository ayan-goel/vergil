// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function approve(address, uint256) external returns (bool);
    function allowance(address, address) external view returns (uint256);
}

contract Check_erc20_approve_conformance {
    ITokenLike public token;

    function check_approve_sets_exact_allowance(
        address spender,
        uint256 value
    ) public {
        try token.approve(spender, value) returns (bool ok) {
            if (ok) {
                assert(token.allowance(address(this), spender) == value);
            }
        } catch {}
    }

    function check_approve_is_last_write_wins(
        address spender,
        uint256 v0,
        uint256 v1
    ) public {
        try token.approve(spender, v0) returns (bool) {} catch { return; }
        try token.approve(spender, v1) returns (bool ok) {
            if (ok) {
                assert(token.allowance(address(this), spender) == v1);
            }
        } catch {}
    }
}
