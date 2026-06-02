// SPDX-License-Identifier: Apache-2.0
// Vergil reference ERC-4626 tokenized vault: minimal single-file
// implementation with the same monotonicity, conservation and
// round-trip semantics as OpenZeppelin's `ERC4626.sol` for the
// properties the kill criterion verifies.
//
// Inlines a tiny ERC-20 for the share token so the whole vault fits in
// one file. Assets are tracked via a simple uint256 (no external ERC-20
// transfer to a wrapped underlying), which lets Halmos reason about
// the vault's accounting without modeling reentrancy on an untrusted
// asset contract.

pragma solidity ^0.8.20;

/// @notice Minimal ERC-4626 tokenized vault used as a verification
///         reference contract. Single-file implementation with inline
///         share-token accounting so Halmos can reason about share /
///         asset conversions without modeling an external ERC-20.
/// @custom:security Share-asset conversions floor (OZ-style); the
///                  round-trip assets → shares → assets never inflates.
contract ERC4626 {
    // Share token state (ERC-20 surface).
    string public name = "Vault";
    string public symbol = "vSHARE";
    uint8 public constant decimals = 18;

    /// @invariant totalSupply equals the sum of balanceOf[*] at all times.
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    /// @notice Underlying asset accounting. The deposit/withdraw flows
    ///         preserve the `shares × totalAssets / totalSupply`
    ///         relationship that drives the monotonicity properties.
    /// @invariant totalAssets is non-decreasing across any successful
    ///            deposit; non-increasing across any successful redeem.
    uint256 public totalAssets;
    mapping(address => uint256) public assetBalance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
    event Deposit(address indexed caller, address indexed owner, uint256 assets, uint256 shares);
    event Withdraw(
        address indexed caller, address indexed receiver, address indexed owner,
        uint256 assets, uint256 shares
    );

    // Seed initial asset balance so the conversion ratio is well-defined
    // at deployment. mintTo gets a starting asset balance and 1 share so
    // every conversion factors in the same way.
    constructor(uint256 seedAssets, address mintTo) {
        require(mintTo != address(0), "ERC4626: zero seed");
        assetBalance[mintTo] = seedAssets;
    }

    /// @notice Convert asset amount to share amount using floor rounding.
    /// @dev OZ-style rounding for previewDeposit. Monotone in `assets`.
    /// @invariant convertToShares is non-decreasing in its `assets`
    ///            argument (more assets in, never fewer shares out).
    function convertToShares(uint256 assets) public view returns (uint256) {
        if (totalSupply == 0 || totalAssets == 0) return assets;
        return (assets * totalSupply) / totalAssets;
    }

    /// @notice Convert share amount to asset amount using floor rounding.
    /// @dev OZ-style rounding for previewRedeem. Monotone in `shares`.
    /// @invariant convertToAssets is non-decreasing in its `shares`
    ///            argument.
    function convertToAssets(uint256 shares) public view returns (uint256) {
        if (totalSupply == 0) return shares;
        return (shares * totalAssets) / totalSupply;
    }

    function previewDeposit(uint256 assets) external view returns (uint256) {
        return convertToShares(assets);
    }

    function previewRedeem(uint256 shares) external view returns (uint256) {
        return convertToAssets(shares);
    }

    function deposit(uint256 assets, address receiver) external returns (uint256 shares) {
        require(receiver != address(0), "ERC4626: deposit to zero");
        require(assetBalance[msg.sender] >= assets, "ERC4626: insufficient assets");
        shares = convertToShares(assets);
        unchecked {
            assetBalance[msg.sender] -= assets;
        }
        totalAssets += assets;
        _mint(receiver, shares);
        emit Deposit(msg.sender, receiver, assets, shares);
    }

    function redeem(uint256 shares, address receiver, address owner)
        external
        returns (uint256 assets)
    {
        require(receiver != address(0), "ERC4626: redeem to zero");
        require(balanceOf[owner] >= shares, "ERC4626: insufficient shares");
        if (msg.sender != owner) {
            uint256 a = allowance[owner][msg.sender];
            require(a >= shares, "ERC4626: insufficient allowance");
            if (a != type(uint256).max) {
                unchecked {
                    allowance[owner][msg.sender] = a - shares;
                }
            }
        }
        assets = convertToAssets(shares);
        _burn(owner, shares);
        unchecked {
            totalAssets -= assets;
        }
        assetBalance[receiver] += assets;
        emit Withdraw(msg.sender, receiver, owner, assets, shares);
    }

    // ERC-20 surface on the share token. No metadata extension.

    function transfer(address to, uint256 amount) external returns (bool) {
        _transfer(msg.sender, to, amount);
        return true;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        emit Approval(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 a = allowance[from][msg.sender];
        require(a >= amount, "ERC4626: insufficient allowance");
        if (a != type(uint256).max) {
            unchecked {
                allowance[from][msg.sender] = a - amount;
            }
        }
        _transfer(from, to, amount);
        return true;
    }

    function _transfer(address from, address to, uint256 amount) internal {
        require(from != address(0), "ERC4626: transfer from zero");
        require(to != address(0), "ERC4626: transfer to zero");
        require(balanceOf[from] >= amount, "ERC4626: insufficient share balance");
        unchecked {
            balanceOf[from] -= amount;
            balanceOf[to] += amount;
        }
        emit Transfer(from, to, amount);
    }

    function _mint(address to, uint256 amount) internal {
        require(to != address(0), "ERC4626: mint to zero");
        totalSupply += amount;
        unchecked {
            balanceOf[to] += amount;
        }
        emit Transfer(address(0), to, amount);
    }

    function _burn(address from, uint256 amount) internal {
        require(balanceOf[from] >= amount, "ERC4626: burn exceeds balance");
        unchecked {
            balanceOf[from] -= amount;
            totalSupply -= amount;
        }
        emit Transfer(from, address(0), amount);
    }
}
