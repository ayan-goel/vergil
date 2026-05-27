// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {ERC721} from "../src/ERC721.sol";

// Hand-written Halmos check_ functions for examples/erc721.
// Each function captures a conformance property that Halmos can discharge
// symbolically. Sourced from the kill-criterion ground truth set; the
// per-property targeted-intent runner verifies these from natural-language
// intents — they're inlined here for the deterministic Phase 1 path.
contract Properties {
    ERC721 internal token;

    constructor() {
        token = new ERC721();
    }

    /// ownerOf(tokenId) either reverts (token doesn't exist) or returns a
    /// non-zero address. Verifying the second branch is the property.
    function check_owner_is_nonzero_for_existing_token(uint256 tokenId) external view {
        try token.ownerOf(tokenId) returns (address owner) {
            assert(owner != address(0));
        } catch {
            // Token doesn't exist — vacuously satisfies the post-condition.
        }
    }

    /// balanceOf(address(0)) must always revert.
    function check_balance_of_zero_reverts() external view {
        try token.balanceOf(address(0)) returns (uint256) {
            assert(false);
        } catch {
            // Expected: must revert.
        }
    }

    /// A successful mint(to, id) sets ownerOf(id) to `to` and increments
    /// balanceOf(to) by exactly 1.
    function check_mint_sets_owner_and_increments_balance(address to, uint256 tokenId) external {
        require(to != address(0));
        uint256 balBefore = token.balanceOf(to);
        try token.mint(to, tokenId) {
            assert(token.ownerOf(tokenId) == to);
            assert(token.balanceOf(to) == balBefore + 1);
        } catch {
            // Mint can revert (already minted, to == 0, etc.); only assert when it succeeds.
        }
    }

    /// setApprovalForAll(operator, flag) records `flag` in the public
    /// isApprovedForAll mapping when the operator is not the caller.
    function check_setApprovalForAll_records_the_flag(address operator, bool approved) external {
        require(operator != address(this));
        try token.setApprovalForAll(operator, approved) {
            assert(token.isApprovedForAll(address(this), operator) == approved);
        } catch {
            // Reverts only when operator == msg.sender (we excluded that case).
        }
    }
}
