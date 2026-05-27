//! Property catalog: typed templates, manifests, and retrieval.
//!
//! Each template is a directory under `templates/` containing:
//!   * `manifest.yaml` — id, description, cost class, declared dependencies
//!     on static-analysis state (storage slots, modifiers, external calls),
//!     pointers to encoding files, and provenance (tier + license).
//!   * `halmos.sol` — Halmos `check_*` function encoding the property.
//!   * `smtchecker.sol` — SMTChecker-compatible source (or empty for
//!     Halmos-only properties).
//!
//! [`Catalog::load`] walks a templates directory, parses every manifest,
//! reads the encoding files, and runs a structural lint that rejects:
//!   - missing encoding files,
//!   - GPL/AGPL/BUSL licensed content in Tier 1 / Tier 2 (forbidden by
//!     SPEC §3.9 — Vergil distributes the catalog as part of the binary),
//!   - manifests whose declared file paths escape the template directory.

pub mod catalog;

pub use catalog::{
    Catalog, CostClass, EncodingPaths, PropertyManifest, PropertyTemplate, Provenance,
    StorageSlotReq, TemplateError, Tier,
};
