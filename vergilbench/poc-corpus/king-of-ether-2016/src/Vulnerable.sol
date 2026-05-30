// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// King of the Ether Throne (Feb 2016) — earliest documented
/// push-payment DoS.
///
/// Reproduction note: when a new monarch claimed the throne, the
/// contract attempted to compensate the previous king via
/// `compensateClaimant`'s push-payment. If the previous king was a
/// contract whose receive() reverted (or exceeded the 2300-gas
/// transfer stipend), the entire compensateClaimant call failed and
/// the throne couldn't be claimed. Catalog template
/// `dos-push-payment-failure` encodes the pull-payment mitigation
/// (each recipient withdraws their own balance).
contract KingOfTheEther {
    mapping(address => uint256) public credited;

    /// Bug: push-payment style. If `a` reverts on receive, the whole
    /// distribute() reverts and `b` never gets credited either.
    function distribute(address a, address b, uint256 amount) external {
        (bool ok1, ) = a.call("");
        require(ok1, "KingOfTheEther: previous monarch refused");
        credited[a] += amount;
        (bool ok2, ) = b.call("");
        require(ok2, "KingOfTheEther: new monarch refused");
        credited[b] += amount;
    }
}
