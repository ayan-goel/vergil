// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IAmmLike {
    function totalSupply() external view returns (uint256);
    function mint(uint256 depositX, uint256 depositY, address to) external returns (uint256);
}

contract Check_amm_mint_increases_supply {
    IAmmLike public token;

    function check_mint_increases_totalSupply(uint256 depositX, uint256 depositY, address to) public {
        require(to != address(0));
        require(depositX > 0 && depositX <= type(uint64).max);
        require(depositY > 0 && depositY <= type(uint64).max);
        uint256 before = token.totalSupply();
        try token.mint(depositX, depositY, to) returns (uint256) {
            assert(token.totalSupply() > before);
        } catch {
            // Mint may revert on non-proportional deposit; only assert on success.
        }
    }
}
