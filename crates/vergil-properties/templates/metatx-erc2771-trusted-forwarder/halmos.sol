// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IERC2771Like {
    function isTrustedForwarder(address forwarder) external view returns (bool);
}

contract Check_metatx_erc2771_trusted_forwarder {
    IERC2771Like internal target;
    address internal trustedForwarder;

    function check_only_configured_forwarder(address other) external view {
        require(other != trustedForwarder);
        assert(target.isTrustedForwarder(trustedForwarder));
        assert(!target.isTrustedForwarder(other));
    }
}
