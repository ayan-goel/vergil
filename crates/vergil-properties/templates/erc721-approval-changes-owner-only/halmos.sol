// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc721Like {
    function ownerOf(uint256) external view returns (address);
    function isApprovedForAll(address, address) external view returns (bool);
    function approve(address, uint256) external;
}

contract Check_erc721_approval_changes_owner_only {
    IErc721Like public token;

    function check_unauthorized_approve_reverts(
        address spender,
        uint256 tokenId
    ) public {
        address owner;
        try token.ownerOf(tokenId) returns (address o) { owner = o; } catch { return; }
        require(owner != address(this));
        require(!token.isApprovedForAll(owner, address(this)));
        try token.approve(spender, tokenId) {
            assert(false);
        } catch {}
    }
}
