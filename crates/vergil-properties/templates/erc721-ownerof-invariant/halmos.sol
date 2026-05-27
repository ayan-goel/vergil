// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc721Like {
    function ownerOf(uint256) external view returns (address);
    function balanceOf(address) external view returns (uint256);
}

contract Check_erc721_ownerof_invariant {
    IErc721Like public token;

    function check_owner_is_nonzero_for_existing_token(uint256 tokenId) public {
        try token.ownerOf(tokenId) returns (address owner) {
            assert(owner != address(0));
        } catch {}
    }

    function check_owner_read_is_deterministic(uint256 tokenId) public view {
        address a;
        address b;
        try token.ownerOf(tokenId) returns (address x) {
            a = x;
        } catch { return; }
        try token.ownerOf(tokenId) returns (address y) {
            b = y;
        } catch { return; }
        assert(a == b);
    }
}
