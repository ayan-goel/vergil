// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc721Like {
    function balanceOf(address) external view returns (uint256);
}

contract Check_erc721_balance_of_non_owners {
    IErc721Like public token;

    function check_balance_of_zero_reverts() public view {
        try token.balanceOf(address(0)) returns (uint256) {
            assert(false);
        } catch {}
    }
}
