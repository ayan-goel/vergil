# DVL Hash-collisions — first vendored PoC

The vulnerable contract (`HashCollisionBug`) is vendored verbatim from
DeFiVulnLabs (MIT licensed). This is the **first author-independent
reproduction in the V1.5 corpus** — the other 10 PoCs are all written by
the catalog author.

**Bug class:** `abi.encodePacked` over multiple dynamic-length values
produces collisions. The canonical example: `("AAA","BBB")` and
`("AA","ABBB")` packed-concatenate to the same byte sequence
`0x414141 || 0x424242` = `0x414141424242`, hashing identically. Our
template uses the equivalent pair `("aa","bbcc")` vs `("aabb","cc")`.

**SWC-133.** See [DVL Hash-collisions.sol](https://github.com/SunWeb3Sec/DeFiVulnLabs/blob/main/src/test/Hash-collisions.sol)
for the original reduction.

**Maps to:** `quirk-abi-encode-packed-collision`.

**Adapter caveat:** Vergil's template hardcodes `target.identify(bytes,bytes)`.
DVL's contract exposes `createHash(string,string)`. A thin `Target`
adapter wraps the vendored contract to expose the template's binding
surface. The adapter cannot introduce or hide the collision — Halmos's
cex either reaches DVL's verbatim `createHash` or it doesn't.

The adapter is a real V2 gap (template binding rigidity); the workaround
keeps this PoC viable while the template refactor is queued.
