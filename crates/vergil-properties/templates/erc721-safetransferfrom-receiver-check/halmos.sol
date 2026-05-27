// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc721Like {
    function safeTransferFrom(address, address, uint256) external;
    function ownerOf(uint256) external view returns (address);
}

// Helper receiver that does NOT implement onERC721Received correctly.
contract BadReceiver {}

contract Check_erc721_safetransferfrom_receiver_check {
    IErc721Like public token;

    function check_safeTransfer_to_bad_receiver_reverts(
        address from,
        uint256 tokenId
    ) public {
        BadReceiver bad = new BadReceiver();
        try token.safeTransferFrom(from, address(bad), tokenId) {
            // Owner must not have moved to the bad receiver.
            assert(token.ownerOf(tokenId) != address(bad));
        } catch {}
    }
}
