// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC165Like {
    function supportsInterface(bytes4 interfaceId) external view returns (bool);
}

contract Check_util_erc165_supports_self {
    IERC165Like internal target;

    function check_erc165_self_and_invalid() external view {
        assert(target.supportsInterface(0x01ffc9a7));
        assert(!target.supportsInterface(0xffffffff));
    }
}
