// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ISoulboundLike {
    function mint(address to, uint256 tokenId) external;
    function transferFrom(address from, address to, uint256 tokenId) external;
}

contract Check_erc721_soulbound_blocks_transfer {
    ISoulboundLike internal token;

    function check_transfer_blocked(uint256 tokenId) external {
        token.mint(address(this), tokenId);
        try token.transferFrom(address(this), address(0xBEEF), tokenId) {
            assert(false);
        } catch {}
    }
}
