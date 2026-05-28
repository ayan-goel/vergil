// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC1155PausableLike {
    function pause() external;
    function mint(address to, uint256 id, uint256 amount) external;
}

contract Check_erc1155_pausable_blocks_transfer {
    IERC1155PausableLike internal token;

    function check_paused_blocks_mint(address to, uint256 id, uint256 amount) external {
        require(to != address(0));
        token.pause();
        try token.mint(to, id, amount) { assert(false); } catch {}
    }
}
