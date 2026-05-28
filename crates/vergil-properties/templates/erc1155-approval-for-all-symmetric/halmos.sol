// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC1155ApprovalLike {
    function setApprovalForAll(address operator, bool approved) external;
    function isApprovedForAll(address account, address operator) external view returns (bool);
}

contract Check_erc1155_approval_for_all_symmetric {
    IERC1155ApprovalLike internal token;

    function check_set_approval_for_all(address operator, bool approved) external {
        token.setApprovalForAll(operator, approved);
        assert(token.isApprovedForAll(address(this), operator) == approved);
    }
}
