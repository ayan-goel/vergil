// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/// BeautyChain / BEC (Apr 2018) — batchOverflow (CVE-2018-10299).
///
/// Reproduction note: BEC's `batchTransfer(address[] _receivers, uint256
/// _value)` computed `_receivers.length * _value` without overflow
/// check in Solidity 0.4.x. With a length of 2 and a `_value` of
/// `2^255`, the result wrapped to 0 — the subsequent
/// `require(balanceOf[msg.sender] >= amount)` succeeded against a
/// zero requirement, then the loop credited each receiver `2^255`
/// tokens.
///
/// This minimal reproduction isolates the unchecked multiplication
/// as `batchAmount(a, b)` and keeps an `amounts` mapping to model
/// the receiver-credit surface. Halmos verifies the negation
/// `result >= a && result >= b` (any clean multiplication satisfies
/// this), refuted by the unchecked wrap.
contract BeautyChainToken {
    mapping(address => uint256) public balances;

    /// The vulnerable arithmetic kernel — preserved from BEC's
    /// `batchTransfer`'s product calculation.
    function batchAmount(uint256 a, uint256 b) external pure returns (uint256) {
        unchecked {
            return a * b;
        }
    }
}
