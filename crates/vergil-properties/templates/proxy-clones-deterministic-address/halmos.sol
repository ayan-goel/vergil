// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ICloneFactoryLike {
    function predict(bytes32 salt) external view returns (address);
    function cloneDet(bytes32 salt) external returns (address);
}

contract Check_proxy_clones_deterministic_address {
    ICloneFactoryLike internal factory;

    function check_clone_matches_prediction(bytes32 salt) external {
        address predicted = factory.predict(salt);
        address actual = factory.cloneDet(salt);
        assert(actual == predicted);
    }
}
