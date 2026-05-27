// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function balanceOf(address) external view returns (uint256);
    function allowance(address, address) external view returns (uint256);
    function transferFrom(address, address, uint256) external returns (bool);
}

contract Check_erc20_transferfrom_conformance {
    ITokenLike public token;

    function check_transferFrom_blocks_unauthorized(
        address from,
        address to,
        uint256 amount
    ) public {
        require(from != address(this));
        require(amount > 0);
        require(token.allowance(from, address(this)) < amount);
        try token.transferFrom(from, to, amount) returns (bool ok) {
            assert(!ok);
        } catch {}
    }

    function check_transferFrom_decrements_finite_allowance(
        address from,
        address to,
        uint256 amount
    ) public {
        uint256 a0 = token.allowance(from, address(this));
        require(a0 < type(uint256).max);
        require(amount > 0 && amount <= a0);
        require(token.balanceOf(from) >= amount);
        try token.transferFrom(from, to, amount) returns (bool ok) {
            if (ok) {
                assert(token.allowance(from, address(this)) == a0 - amount);
            }
        } catch {}
    }

    function check_transferFrom_preserves_infinite_allowance(
        address from,
        address to,
        uint256 amount
    ) public {
        require(token.allowance(from, address(this)) == type(uint256).max);
        require(token.balanceOf(from) >= amount);
        try token.transferFrom(from, to, amount) returns (bool ok) {
            if (ok) {
                assert(token.allowance(from, address(this)) == type(uint256).max);
            }
        } catch {}
    }
}
