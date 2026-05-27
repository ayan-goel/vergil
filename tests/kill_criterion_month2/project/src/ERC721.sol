// SPDX-License-Identifier: Apache-2.0
// Vergil reference ERC-721: minimal single-file implementation with the
// same on-chain semantics as OpenZeppelin's `ERC721.sol` for the
// properties the kill criterion verifies (ownerOf invariant,
// transferFrom-clears-approval, zero-address rejection, mint accounting).
//
// Omitted: ERC165, IERC721Receiver onReceive callback (safeTransferFrom),
// tokenURI metadata. None of those are part of the conformance
// properties Phase 2 targets.

pragma solidity ^0.8.20;

contract ERC721 {
    // Token id → owner. address(0) means token does not exist.
    mapping(uint256 => address) private _owners;
    // Owner → balance.
    mapping(address => uint256) private _balances;
    // Token id → approved address.
    mapping(uint256 => address) private _tokenApprovals;
    // Owner → operator → approved-for-all flag.
    mapping(address => mapping(address => bool)) public isApprovedForAll;

    event Transfer(address indexed from, address indexed to, uint256 indexed tokenId);
    event Approval(address indexed owner, address indexed approved, uint256 indexed tokenId);
    event ApprovalForAll(address indexed owner, address indexed operator, bool approved);

    function ownerOf(uint256 tokenId) public view returns (address) {
        address o = _owners[tokenId];
        require(o != address(0), "ERC721: nonexistent token");
        return o;
    }

    function balanceOf(address owner) public view returns (uint256) {
        require(owner != address(0), "ERC721: balance of zero");
        return _balances[owner];
    }

    function getApproved(uint256 tokenId) public view returns (address) {
        require(_owners[tokenId] != address(0), "ERC721: nonexistent token");
        return _tokenApprovals[tokenId];
    }

    function approve(address to, uint256 tokenId) external {
        address owner = ownerOf(tokenId);
        require(
            msg.sender == owner || isApprovedForAll[owner][msg.sender],
            "ERC721: not authorized"
        );
        require(to != owner, "ERC721: approve to current owner");
        _tokenApprovals[tokenId] = to;
        emit Approval(owner, to, tokenId);
    }

    function setApprovalForAll(address operator, bool approved) external {
        require(operator != msg.sender, "ERC721: approve to caller");
        isApprovedForAll[msg.sender][operator] = approved;
        emit ApprovalForAll(msg.sender, operator, approved);
    }

    function transferFrom(address from, address to, uint256 tokenId) external {
        require(_isAuthorized(msg.sender, tokenId), "ERC721: not authorized");
        require(to != address(0), "ERC721: transfer to zero");
        require(_owners[tokenId] == from, "ERC721: wrong from");
        // Clear the per-token approval — this is part of the conformance.
        _tokenApprovals[tokenId] = address(0);
        unchecked {
            _balances[from] -= 1;
            _balances[to] += 1;
        }
        _owners[tokenId] = to;
        emit Transfer(from, to, tokenId);
    }

    function mint(address to, uint256 tokenId) external {
        require(to != address(0), "ERC721: mint to zero");
        require(_owners[tokenId] == address(0), "ERC721: already minted");
        unchecked {
            _balances[to] += 1;
        }
        _owners[tokenId] = to;
        emit Transfer(address(0), to, tokenId);
    }

    function _isAuthorized(address spender, uint256 tokenId) internal view returns (bool) {
        address owner = _owners[tokenId];
        if (owner == address(0)) return false;
        return spender == owner
            || isApprovedForAll[owner][spender]
            || _tokenApprovals[tokenId] == spender;
    }
}
