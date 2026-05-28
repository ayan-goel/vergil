// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {EIP712} from "@openzeppelin/contracts/utils/cryptography/EIP712.sol";

contract Contract is EIP712 {
    constructor() EIP712("MyDomain", "1") {}
    function separator() external view returns (bytes32) { return _domainSeparatorV4(); }
}
