// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IFlashLike {
    function flashFee(address token, uint256 amount) external view returns (uint256);
}

contract Check_erc20_flashmint_fee_default_zero {
    IFlashLike internal token;

    function check_default_flash_fee_zero(uint256 amount) external view {
        assert(token.flashFee(address(token), amount) == 0);
    }
}
