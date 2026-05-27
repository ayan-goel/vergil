// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc721Like {
    function ownerOf(uint256 tokenId) external view returns (address);
    function isApprovedForAll(address owner, address operator) external view returns (bool);
    function approve(address to, uint256 tokenId) external;
}

contract Check_erc721_unauthorized_approve_reverts {
    IErc721Like public token;

    function check_unauthorized_approve_reverts(address spender, uint256 tokenId) public {
        address owner;
        try token.ownerOf(tokenId) returns (address o) {
            owner = o;
        } catch {
            return; // Token doesn't exist — approve would revert anyway.
        }
        require(owner != address(0));
        require(address(this) != owner);
        require(!token.isApprovedForAll(owner, address(this)));
        try token.approve(spender, tokenId) {
            assert(false);
        } catch {
            // Expected.
        }
    }
}
