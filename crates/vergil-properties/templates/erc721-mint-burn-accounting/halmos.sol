// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc721MintBurnLike {
    function ownerOf(uint256) external view returns (address);
    function balanceOf(address) external view returns (uint256);
    function mint(address, uint256) external;
    function burn(uint256) external;
}

contract Check_erc721_mint_burn_accounting {
    IErc721MintBurnLike public token;

    function check_mint_sets_owner_and_increments_balance(address to, uint256 tokenId) public {
        require(to != address(0));
        uint256 b0 = token.balanceOf(to);
        try token.mint(to, tokenId) {
            assert(token.ownerOf(tokenId) == to);
            assert(token.balanceOf(to) == b0 + 1);
        } catch {}
    }

    function check_burn_clears_owner_and_decrements_balance(uint256 tokenId) public {
        address owner;
        try token.ownerOf(tokenId) returns (address o) { owner = o; } catch { return; }
        uint256 b0 = token.balanceOf(owner);
        try token.burn(tokenId) {
            // post-burn ownerOf should revert
            try token.ownerOf(tokenId) returns (address) {
                assert(false);
            } catch {
                assert(token.balanceOf(owner) == b0 - 1);
            }
        } catch {}
    }
}
