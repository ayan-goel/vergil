// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface ITokenLike {
    function transfer(address, uint256) external returns (bool);
    function transferFrom(address, address, uint256) external returns (bool);
}

contract Check_erc20_zero_address_rejection {
    ITokenLike public token;

    function check_transfer_to_zero_reverts(uint256 amount) public {
        require(amount > 0);
        try token.transfer(address(0), amount) returns (bool ok) {
            assert(!ok);
        } catch {}
    }

    function check_transferFrom_to_zero_reverts(address from, uint256 amount) public {
        require(amount > 0);
        try token.transferFrom(from, address(0), amount) returns (bool ok) {
            assert(!ok);
        } catch {}
    }
}
