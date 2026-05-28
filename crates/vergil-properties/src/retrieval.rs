//! Top-k template retrieval over the property catalog.
//!
//! The pipeline:
//!   1. [`Retriever::new`] embeds every template's `id + description +
//!      first ~512 chars of halmos source` once, persisting vectors to a
//!      SQLite cache keyed by `(template_id, embedder_id, content_sha)`.
//!   2. [`Retriever::retrieve(intent, k)`] embeds the user's intent string,
//!      computes cosine similarity against every cached vector, and returns
//!      the top-k templates ranked by similarity.
//!
//! Cosine similarity reduces to a dot product because the embedder produces
//! unit-length vectors (both `MockEmbedder` and Voyage normalize by default).

use std::path::Path;

use rusqlite::{params, Connection};
use thiserror::Error;

use crate::catalog::{Catalog, PropertyTemplate};
use crate::embed::{EmbedError, Embedder};

#[derive(Debug, Error)]
pub enum RetrievalError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("embedder: {0}")]
    Embed(#[from] EmbedError),
    #[error("cache dim mismatch: embedder produces {embedder} but cache has {cache}")]
    DimMismatch { embedder: usize, cache: usize },
}

#[derive(Debug, Clone)]
pub struct RetrievedTemplate {
    pub template_id: String,
    pub score: f32,
}

pub struct Retriever {
    catalog: Catalog,
    embedder: Box<dyn Embedder>,
    /// Per-template embedding, in catalog iteration order.
    vectors: Vec<(String, Vec<f32>)>,
}

impl Retriever {
    /// Build a retriever over `catalog`, embedding every template (or
    /// pulling cached vectors from `<cache_dir>/embeddings.sqlite`).
    pub async fn new(
        catalog: Catalog,
        embedder: Box<dyn Embedder>,
        cache_dir: impl AsRef<Path>,
    ) -> Result<Self, RetrievalError> {
        let cache_dir = cache_dir.as_ref();
        std::fs::create_dir_all(cache_dir).map_err(|e| {
            RetrievalError::Sqlite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(format!("create cache dir {}: {e}", cache_dir.display())),
            ))
        })?;
        let db = Connection::open(cache_dir.join("embeddings.sqlite"))?;
        ensure_schema(&db)?;

        let mut vectors = Vec::with_capacity(catalog.len());
        let mut to_embed: Vec<(String, String, [u8; 32])> = Vec::new();
        for tmpl in catalog.iter() {
            let sha = tmpl.content_sha();
            if let Some(v) = read_cached(&db, &tmpl.manifest.id, embedder.id(), &sha)? {
                if v.len() != embedder.dim() {
                    return Err(RetrievalError::DimMismatch {
                        embedder: embedder.dim(),
                        cache: v.len(),
                    });
                }
                vectors.push((tmpl.manifest.id.clone(), v));
            } else {
                to_embed.push((tmpl.manifest.id.clone(), embed_text(tmpl), sha));
            }
        }

        if !to_embed.is_empty() {
            let texts: Vec<String> = to_embed.iter().map(|(_, t, _)| t.clone()).collect();
            let embeds = embedder.embed(&texts).await?;
            for ((id, _, sha), v) in to_embed.into_iter().zip(embeds.into_iter()) {
                if v.len() != embedder.dim() {
                    return Err(RetrievalError::DimMismatch {
                        embedder: embedder.dim(),
                        cache: v.len(),
                    });
                }
                write_cached(&db, &id, embedder.id(), &sha, &v)?;
                vectors.push((id, v));
            }
        }

        Ok(Self {
            catalog,
            embedder,
            vectors,
        })
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// Embed `intent` and return the top-k template IDs ranked by cosine
    /// similarity. Vectors are unit-length so cosine == dot product.
    pub async fn retrieve(
        &self,
        intent: &str,
        k: usize,
    ) -> Result<Vec<RetrievedTemplate>, RetrievalError> {
        self.retrieve_for_interfaces(intent, k, &[]).await
    }

    /// Like [`retrieve`](Self::retrieve), but first restricts the candidate
    /// set to templates applicable to `interfaces`, then ranks by similarity.
    ///
    /// A template is *applicable* when its `applies_to.interfaces` is empty
    /// (universal), contains `Generic`, or intersects `interfaces`
    /// (case-insensitive). When `interfaces` is empty, no filtering happens —
    /// identical to [`retrieve`](Self::retrieve).
    ///
    /// This is the A1 fix. Without it, an ERC-721 intent embeds close to the
    /// catalog's dominant ERC-20 templates (shared `transfer` / `approve` /
    /// `balance` vocabulary) and the synthesizer follows template gravity into
    /// the wrong standard — the documented cause of the ERC-721 kill-criterion
    /// stragglers. Filtering by the contract's detected interface stops the
    /// cross-standard leakage before ranking.
    ///
    /// If the filter would leave an empty candidate set, it falls back to the
    /// unfiltered ranking — some hints beat none.
    pub async fn retrieve_for_interfaces(
        &self,
        intent: &str,
        k: usize,
        interfaces: &[String],
    ) -> Result<Vec<RetrievedTemplate>, RetrievalError> {
        let qv = self.embedder.embed(&[intent.to_string()]).await?;
        let query = qv
            .into_iter()
            .next()
            .ok_or_else(|| RetrievalError::Embed(EmbedError::Schema("empty embedding".into())))?;

        let eligible = |id: &str| -> bool {
            if interfaces.is_empty() {
                return true;
            }
            match self.catalog.get(id) {
                Some(t) => interface_compatible(&t.manifest.applies_to.interfaces, interfaces),
                None => true,
            }
        };

        let mut scored: Vec<RetrievedTemplate> = self
            .vectors
            .iter()
            .filter(|(id, _)| eligible(id))
            .map(|(id, v)| RetrievedTemplate {
                template_id: id.clone(),
                score: dot(&query, v),
            })
            .collect();

        // Fallback: an interface filter that pruned everything is worse than
        // no filter — rank the full set instead.
        if scored.is_empty() && !interfaces.is_empty() {
            scored = self
                .vectors
                .iter()
                .map(|(id, v)| RetrievedTemplate {
                    template_id: id.clone(),
                    score: dot(&query, v),
                })
                .collect();
        }

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(k);
        Ok(scored)
    }

    pub fn template(&self, id: &str) -> Option<&PropertyTemplate> {
        self.catalog.get(id)
    }
}

fn embed_text(t: &PropertyTemplate) -> String {
    // ~512 chars of halmos source is enough to disambiguate semantically;
    // the full source would push longer templates past Voyage's per-input
    // token limit (~120 tokens for voyage-3) without adding signal.
    let halmos_snippet: String = t.halmos_source.chars().take(512).collect();
    format!(
        "{}\n{}\n{}",
        t.manifest.id, t.manifest.description, halmos_snippet
    )
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// A template applies to a target contract when it declares no interface
/// (universal), declares `Generic`, or shares at least one interface tag with
/// the target (case-insensitive).
fn interface_compatible(template_ifaces: &[String], target: &[String]) -> bool {
    if template_ifaces.is_empty() {
        return true;
    }
    if template_ifaces
        .iter()
        .any(|i| i.eq_ignore_ascii_case("Generic"))
    {
        return true;
    }
    template_ifaces
        .iter()
        .any(|ti| target.iter().any(|t| t.eq_ignore_ascii_case(ti)))
}

fn ensure_schema(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS template_embeddings (
            template_id TEXT NOT NULL,
            embedder_id TEXT NOT NULL,
            content_sha BLOB NOT NULL,
            embedding BLOB NOT NULL,
            PRIMARY KEY (template_id, embedder_id)
         );",
    )
}

fn read_cached(
    db: &Connection,
    template_id: &str,
    embedder_id: &str,
    content_sha: &[u8; 32],
) -> rusqlite::Result<Option<Vec<f32>>> {
    let mut stmt = db.prepare(
        "SELECT content_sha, embedding FROM template_embeddings \
         WHERE template_id = ?1 AND embedder_id = ?2",
    )?;
    let mut rows = stmt.query(params![template_id, embedder_id])?;
    if let Some(row) = rows.next()? {
        let stored_sha: Vec<u8> = row.get(0)?;
        if stored_sha.as_slice() == content_sha {
            let bytes: Vec<u8> = row.get(1)?;
            return Ok(Some(decode_vector(&bytes)));
        }
    }
    Ok(None)
}

fn write_cached(
    db: &Connection,
    template_id: &str,
    embedder_id: &str,
    content_sha: &[u8; 32],
    vector: &[f32],
) -> rusqlite::Result<()> {
    let bytes = encode_vector(vector);
    db.execute(
        "INSERT OR REPLACE INTO template_embeddings \
         (template_id, embedder_id, content_sha, embedding) VALUES (?1, ?2, ?3, ?4)",
        params![template_id, embedder_id, content_sha.as_slice(), bytes],
    )?;
    Ok(())
}

fn encode_vector(v: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(v).to_vec()
}

fn decode_vector(bytes: &[u8]) -> Vec<f32> {
    bytemuck::cast_slice(bytes).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::MockEmbedder;

    fn build_test_catalog() -> Catalog {
        // Use the committed templates dir.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        Catalog::load(dir).expect("catalog")
    }

    #[tokio::test]
    async fn first_load_embeds_all_templates() {
        let cat = build_test_catalog();
        let n = cat.len();
        // Phase 4 Slice A7 acceptance: the V1 catalog ships at least 100 templates.
        assert!(n >= 100, "catalog should hold >= 100 templates, found {n}");
        let tmp = tempfile::tempdir().unwrap();
        let r = Retriever::new(cat, Box::new(MockEmbedder::new("test-32", 32)), tmp.path())
            .await
            .unwrap();
        assert_eq!(r.vectors.len(), n, "all templates embedded");
    }

    #[tokio::test]
    async fn second_load_hits_cache() {
        // First load populates the cache; second load constructs with a
        // failing-embedder. If the cache works, embed() is never called
        // and construction succeeds.
        let tmp = tempfile::tempdir().unwrap();
        let cat = build_test_catalog();
        {
            let _r = Retriever::new(
                cat.clone(),
                Box::new(MockEmbedder::new("test-32", 32)),
                tmp.path(),
            )
            .await
            .unwrap();
        }

        struct ExplodingEmbedder;
        #[async_trait::async_trait]
        impl Embedder for ExplodingEmbedder {
            fn id(&self) -> &str {
                "test-32"
            }
            fn dim(&self) -> usize {
                32
            }
            async fn embed(&self, _inputs: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
                panic!(
                    "embed should not have been called — cache should have served all templates"
                );
            }
        }

        let _r2 = Retriever::new(cat, Box::new(ExplodingEmbedder), tmp.path())
            .await
            .expect("cache served everything");
    }

    #[tokio::test]
    async fn retrieve_returns_topk_sorted_by_score() {
        let tmp = tempfile::tempdir().unwrap();
        let cat = build_test_catalog();
        let r = Retriever::new(cat, Box::new(MockEmbedder::new("test-32", 32)), tmp.path())
            .await
            .unwrap();
        let hits = r.retrieve("ERC20 transferFrom allowance", 8).await.unwrap();
        assert_eq!(hits.len(), 8);
        for w in hits.windows(2) {
            assert!(w[0].score >= w[1].score, "not sorted desc: {hits:?}");
        }
        // Self-similarity: the template whose content text most closely
        // matches the query should be in the top-k. With hash-based mock
        // embeddings there's no semantic guarantee, but we can at least
        // assert all hits are real template ids from the catalog.
        for h in &hits {
            assert!(r.template(&h.template_id).is_some());
        }
    }

    #[tokio::test]
    async fn dim_mismatch_is_error() {
        let tmp = tempfile::tempdir().unwrap();
        let cat = build_test_catalog();
        let _ = Retriever::new(
            cat.clone(),
            Box::new(MockEmbedder::new("test-32", 32)),
            tmp.path(),
        )
        .await
        .expect("first load ok");
        // Re-use same embedder_id with a different dim — cache lookup returns
        // a 32-dim vector while the embedder claims 64, so DimMismatch fires.
        match Retriever::new(cat, Box::new(MockEmbedder::new("test-32", 64)), tmp.path()).await {
            Err(RetrievalError::DimMismatch { embedder, cache }) => {
                assert_eq!(embedder, 64);
                assert_eq!(cache, 32);
            }
            Err(other) => panic!("expected DimMismatch, got {other}"),
            Ok(_) => panic!("expected dim mismatch error"),
        }
    }

    #[tokio::test]
    async fn retrieve_for_interfaces_excludes_cross_standard_templates() {
        let tmp = tempfile::tempdir().unwrap();
        let cat = build_test_catalog();
        let r = Retriever::new(cat, Box::new(MockEmbedder::new("test-32", 32)), tmp.path())
            .await
            .unwrap();
        let hits = r
            .retrieve_for_interfaces(
                "clear per-token approval on transfer",
                8,
                &["ERC721".to_string()],
            )
            .await
            .unwrap();
        assert!(!hits.is_empty(), "ERC721 has applicable templates");
        for h in &hits {
            let t = r.template(&h.template_id).expect("hit is a real template");
            let ifaces = &t.manifest.applies_to.interfaces;
            let ok = ifaces.is_empty()
                || ifaces.iter().any(|i| i.eq_ignore_ascii_case("Generic"))
                || ifaces.iter().any(|i| i.eq_ignore_ascii_case("ERC721"));
            assert!(
                ok,
                "ERC20-only/cross-standard template leaked into ERC721 retrieval: {} {:?}",
                h.template_id, ifaces
            );
        }
    }

    #[tokio::test]
    async fn retrieve_for_interfaces_empty_filter_matches_retrieve() {
        let tmp = tempfile::tempdir().unwrap();
        let cat = build_test_catalog();
        let r = Retriever::new(cat, Box::new(MockEmbedder::new("test-32", 32)), tmp.path())
            .await
            .unwrap();
        let a = r.retrieve("ERC20 transfer", 5).await.unwrap();
        let b = r
            .retrieve_for_interfaces("ERC20 transfer", 5, &[])
            .await
            .unwrap();
        let ids_a: Vec<&str> = a.iter().map(|h| h.template_id.as_str()).collect();
        let ids_b: Vec<&str> = b.iter().map(|h| h.template_id.as_str()).collect();
        assert_eq!(ids_a, ids_b, "empty filter must equal unfiltered retrieve");
    }

    #[test]
    fn interface_compatible_rules() {
        // universal + Generic always pass
        assert!(interface_compatible(&[], &["ERC721".to_string()]));
        assert!(interface_compatible(
            &["Generic".to_string()],
            &["ERC721".to_string()]
        ));
        // exact + partial intersection pass
        assert!(interface_compatible(
            &["ERC721".to_string()],
            &["ERC721".to_string()]
        ));
        assert!(interface_compatible(
            &["ERC20".to_string(), "Pausable".to_string()],
            &["ERC20".to_string()]
        ));
        // disjoint standards do not
        assert!(!interface_compatible(
            &["ERC20".to_string()],
            &["ERC721".to_string()]
        ));
        assert!(!interface_compatible(
            &["ERC4626".to_string()],
            &["ERC721".to_string()]
        ));
    }

    #[test]
    fn encode_decode_vector_roundtrip() {
        let v: Vec<f32> = vec![0.1, 0.2, -0.3, 0.4];
        let bytes = encode_vector(&v);
        let back = decode_vector(&bytes);
        assert_eq!(v, back);
    }
}
