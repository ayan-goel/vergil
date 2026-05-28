// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC721ConsecutiveLike {
    function ownerOf(uint256 tokenId) external view returns (address);
}

contract Check_erc721_consecutive_batch_owner {
    IERC721ConsecutiveLike internal token;
    address internal batchRecipient;
    uint256 internal batchSize;

    function check_consecutive_owner(uint256 tokenId) external view {
        require(tokenId < batchSize);
        assert(token.ownerOf(tokenId) == batchRecipient);
    }
}
