// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Token} from "./Token.sol";

/// Minimal share-token vault that wraps a `Token`. The vault holds the
/// underlying asset and issues shares 1:1 on deposit + redeem; this
/// gives Vergil a clean cross-contract invariant to verify without
/// hitting the nonlinear math that trips Halmos on real-world vaults.
///
/// Cross-contract invariant the property tests pin:
///   vault.totalAssets() == token.balanceOf(address(vault))
contract Vault {
    Token public immutable token;
    mapping(address => uint256) public shares;
    uint256 public totalShares;

    constructor(Token _token) {
        token = _token;
    }

    /// Caller already transferred `amount` of token to the vault.
    /// The vault credits shares 1:1.
    function depositFor(address receiver, uint256 amount) external {
        shares[receiver] += amount;
        totalShares += amount;
    }

    function redeem(uint256 amount) external {
        require(shares[msg.sender] >= amount, "shares");
        unchecked {
            shares[msg.sender] -= amount;
            totalShares -= amount;
        }
        token.transfer(msg.sender, amount);
    }

    /// Live asset balance held by the vault. Should equal `totalShares`
    /// post-construction barring direct token transfers in.
    function totalAssets() external view returns (uint256) {
        return token.balanceOf(address(this));
    }
}
