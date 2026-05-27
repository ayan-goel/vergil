// SPDX-License-Identifier: Apache-2.0
// SMTChecker (CHC) encoding for erc20-sum-of-balances.
//
// The CHC engine tracks a ghost sum updated alongside _balances. The
// invariant `_ghostSum == _totalSupply` should hold across every
// transaction. The asserts below are written so the user's contract
// can inherit and the CHC engine sees them in scope.

pragma solidity ^0.8.0;

abstract contract Erc20SumOfBalancesInvariant {
    mapping(address => uint256) internal _balances;
    uint256 internal _totalSupply;
    uint256 internal _ghostSum;

    function _credit(address to, uint256 amount) internal {
        _balances[to] += amount;
        _ghostSum += amount;
        assert(_ghostSum == _totalSupply || _ghostSum == _totalSupply + amount);
    }

    function _debit(address from, uint256 amount) internal {
        _balances[from] -= amount;
        _ghostSum -= amount;
        assert(_ghostSum == _totalSupply || _ghostSum + amount == _totalSupply);
    }

    function _mint(address to, uint256 amount) internal {
        _totalSupply += amount;
        _credit(to, amount);
    }

    function _burn(address from, uint256 amount) internal {
        _debit(from, amount);
        _totalSupply -= amount;
    }
}
