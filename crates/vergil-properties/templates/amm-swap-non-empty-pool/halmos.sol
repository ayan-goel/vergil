// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IAmmLike {
    function reserveX() external view returns (uint256);
    function reserveY() external view returns (uint256);
    function swapXForY(uint256 amountIn) external returns (uint256);
}

contract Check_amm_swap_non_empty_pool {
    IAmmLike public token;

    function check_swap_does_not_drain_pool(uint256 amountIn) public {
        require(amountIn > 0 && amountIn <= type(uint128).max);
        try token.swapXForY(amountIn) returns (uint256) {
            assert(token.reserveY() > 0);
        } catch {
            // Swap may revert on empty pool or zero output — skip.
        }
    }
}
