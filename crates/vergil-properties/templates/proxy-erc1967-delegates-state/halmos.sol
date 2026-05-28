// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IBoxLike {
    function setValue(uint256 v) external;
    function value() external view returns (uint256);
}

contract Check_proxy_erc1967_delegates_state {
    IBoxLike internal proxy; // cast of the ERC1967Proxy address to the logic ABI

    function check_proxy_delegates_state(uint256 v) external {
        proxy.setValue(v);
        assert(proxy.value() == v);
    }
}
