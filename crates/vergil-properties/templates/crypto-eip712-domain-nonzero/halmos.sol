// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IEip712Like {
    function separator() external view returns (bytes32);
}

contract Check_crypto_eip712_domain_nonzero {
    IEip712Like internal target;

    function check_domain_separator_nonzero_stable() external view {
        bytes32 s = target.separator();
        assert(s != bytes32(0));
        assert(s == target.separator());
    }
}
