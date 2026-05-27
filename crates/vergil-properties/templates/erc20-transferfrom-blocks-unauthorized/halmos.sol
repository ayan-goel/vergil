// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function allowance(address owner, address spender) external view returns (uint256);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}

contract Check_erc20_transferfrom_blocks_unauthorized {
    ITokenLike public token;

    function check_transferFrom_blocks_unauthorized(
        address from,
        address to,
        uint256 amount
    ) public {
        require(from != address(0) && to != address(0));
        uint256 currentAllowance = token.allowance(from, address(this));
        require(currentAllowance != type(uint256).max);
        require(currentAllowance < amount);
        try token.transferFrom(from, to, amount) returns (bool) {
            // Should not succeed when allowance is insufficient.
            assert(false);
        } catch {
            // Expected.
        }
    }
}
