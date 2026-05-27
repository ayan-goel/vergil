// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ILendingLike {
    function collateral(address) external view returns (uint256);
    function debt(address) external view returns (uint256);
    function LTV_BPS() external view returns (uint256);
    function liquidate(address account) external;
}

contract Check_lending_liquidate_only_undercollateralized {
    ILendingLike public token;

    function check_liquidate_reverts_when_solvent(address account) public {
        require(account != address(0));
        uint256 c = token.collateral(account);
        uint256 d = token.debt(account);
        uint256 capacity = (c * token.LTV_BPS()) / 100;
        if (capacity >= d) {
            try token.liquidate(account) {
                assert(false);
            } catch {
                // Expected: cannot liquidate a solvent position.
            }
        }
    }
}
