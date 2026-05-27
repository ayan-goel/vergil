// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IPausableTokenLike {
    function paused() external view returns (bool);
    function transfer(address, uint256) external returns (bool);
    function transferFrom(address, address, uint256) external returns (bool);
}

contract Check_erc20_pausable {
    IPausableTokenLike public token;

    function check_paused_blocks_transfer(address to, uint256 amount) public {
        require(token.paused());
        try token.transfer(to, amount) returns (bool ok) {
            assert(!ok);
        } catch {}
    }

    function check_paused_blocks_transferFrom(address from, address to, uint256 amount) public {
        require(token.paused());
        try token.transferFrom(from, to, amount) returns (bool ok) {
            assert(!ok);
        } catch {}
    }
}
