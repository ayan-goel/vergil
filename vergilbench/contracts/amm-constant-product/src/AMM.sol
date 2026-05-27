// SPDX-License-Identifier: Apache-2.0
// Vergil reference AMM: minimal Uniswap-V2-style constant-product pair
// with inlined uint256 token accounting. Pair behavior:
//   reserveX * reserveY >= k_before, where k_before is taken before each
//   swap and the swap includes a 0.3% fee. The fee makes k strictly
//   non-decreasing — without a fee, swap would preserve k exactly.
//
// Inlines two uint256 reserves and a single LP-share balance map rather
// than wrapping two external ERC-20s, so Halmos can reason about the
// arithmetic without modeling untrusted token contracts.

pragma solidity ^0.8.20;

contract AMM {
    uint256 public reserveX;
    uint256 public reserveY;

    /// LP share token state.
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;

    /// 0.3% fee — fee numerator 997 over denominator 1000.
    uint256 internal constant FEE_NUM = 997;
    uint256 internal constant FEE_DEN = 1000;

    constructor(uint256 initX, uint256 initY) {
        require(initX > 0 && initY > 0, "AMM: zero init");
        reserveX = initX;
        reserveY = initY;
        // Initial LP shares = sqrt(initX * initY) is the OZ rule; we
        // approximate with initX so the conservation logic is testable
        // without a full integer-sqrt encoding.
        totalSupply = initX;
        balanceOf[msg.sender] = initX;
    }

    /// Swap `amountIn` of X for Y. Returns the amount of Y sent out.
    /// Maintains reserveX * reserveY >= kBefore.
    function swapXForY(uint256 amountIn) external returns (uint256 amountOut) {
        require(amountIn > 0, "AMM: zero in");
        uint256 rx = reserveX;
        uint256 ry = reserveY;
        require(rx > 0 && ry > 0, "AMM: empty pool");

        uint256 amountInWithFee = amountIn * FEE_NUM;
        uint256 numerator = amountInWithFee * ry;
        uint256 denominator = rx * FEE_DEN + amountInWithFee;
        amountOut = numerator / denominator;
        require(amountOut > 0 && amountOut < ry, "AMM: bad out");

        reserveX = rx + amountIn;
        reserveY = ry - amountOut;
    }

    /// Mint LP shares proportional to deposit.
    /// shares = (depositX * totalSupply) / reserveX
    function mint(uint256 depositX, uint256 depositY, address to) external returns (uint256 shares) {
        require(to != address(0), "AMM: mint to zero");
        require(depositX > 0 && depositY > 0, "AMM: zero deposit");
        uint256 rx = reserveX;
        uint256 ry = reserveY;
        // Demand proportional deposit. depositY * rx == depositX * ry
        // (within rounding) ensures the LP doesn't dilute the price.
        require(depositY * rx == depositX * ry, "AMM: not proportional");
        shares = (depositX * totalSupply) / rx;
        require(shares > 0, "AMM: zero shares");

        reserveX = rx + depositX;
        reserveY = ry + depositY;
        totalSupply += shares;
        unchecked {
            balanceOf[to] += shares;
        }
    }

    /// Burn LP shares for proportional reserves.
    function burn(uint256 shares, address to) external returns (uint256 outX, uint256 outY) {
        require(to != address(0), "AMM: burn to zero");
        require(shares > 0, "AMM: zero shares");
        require(balanceOf[msg.sender] >= shares, "AMM: insufficient shares");
        uint256 ts = totalSupply;
        require(ts > 0, "AMM: empty supply");

        outX = (shares * reserveX) / ts;
        outY = (shares * reserveY) / ts;
        require(outX > 0 && outY > 0, "AMM: zero out");

        unchecked {
            balanceOf[msg.sender] -= shares;
            totalSupply -= shares;
        }
        reserveX -= outX;
        reserveY -= outY;
    }
}
