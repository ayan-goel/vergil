// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.0;

interface IArithLike {
    function multiply(uint256 a, uint256 b) external pure returns (uint256);
}

contract Check_arith_zero_input_zero_output {
    IArithLike public token;

    function check_zero_input_yields_zero_output(uint256 a) public view {
        require(a <= type(uint128).max);
        // multiply(a, 0) = 0
        assert(token.multiply(a, 0) == 0);
        // multiply(0, a) = 0
        assert(token.multiply(0, a) == 0);
    }
}
