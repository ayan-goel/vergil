// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ICappedTokenLike {
    function totalSupply() external view returns (uint256);
    function cap() external view returns (uint256);
    function mint(address to, uint256 amount) external;
}

contract Check_erc20_supply_cap_respected {
    ICappedTokenLike public token;

    /// After mint(), totalSupply stays at or below cap.
    function check_mint_respects_cap(address to, uint256 amount) external {
        try token.mint(to, amount) {
            assert(token.totalSupply() <= token.cap());
        } catch {}
    }
}
