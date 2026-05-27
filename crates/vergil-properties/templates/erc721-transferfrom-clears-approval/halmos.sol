// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IErc721Like {
    function ownerOf(uint256) external view returns (address);
    function getApproved(uint256) external view returns (address);
    function transferFrom(address, address, uint256) external;
}

contract Check_erc721_transferfrom_clears_approval {
    IErc721Like public token;

    function check_transferFrom_clears_per_token_approval(
        address from,
        address to,
        uint256 tokenId
    ) public {
        require(to != address(0));
        try token.transferFrom(from, to, tokenId) {
            assert(token.getApproved(tokenId) == address(0));
        } catch {}
    }
}
