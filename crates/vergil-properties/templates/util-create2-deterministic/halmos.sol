// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ICreate2Like {
    function computedFor(bytes32 salt, bytes32 codeHash, address deployer) external pure returns (address);
}

contract Check_util_create2_deterministic {
    ICreate2Like internal helper;

    function check_create2_is_deterministic(bytes32 salt, bytes32 codeHash, address deployer) external view {
        assert(
            helper.computedFor(salt, codeHash, deployer)
                == helper.computedFor(salt, codeHash, deployer)
        );
    }
}
