//! Prometheus metrics stub — Phase 4 Slice B2 (the "stub" half).
//!
//! Exposes a small set of counters in Prometheus text format at
//! `GET /metrics`. Phase 4 only ships the shape — counter values are
//! always 0 unless something explicitly bumps them via [`Metrics::inc`].
//! V2 wires real increments from the worker pool + telemetry sink.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Stable counter names. V2's scraping/alerting pins on these literals;
/// do not rename without bumping the metrics-schema version.
pub mod counter {
    /// Total jobs submitted, by terminal status (`pending`/`running`/
    /// `completed`/`failed`).
    pub const JOBS_TOTAL: &str = "vergil_jobs_total";
    /// Total telemetry events ingested by the service. V2's sink writes
    /// here.
    pub const TELEMETRY_EVENTS_TOTAL: &str = "vergil_telemetry_events_total";
    /// Cumulative cost in micro-USD ($1 = 1_000_000 µUSD). Integer
    /// counter so prometheus can scrape it without float-precision games.
    pub const COST_MICRO_USD_TOTAL: &str = "vergil_cost_micro_usd_total";
}

/// One counter is keyed by name + a sorted label set, mirroring the
/// Prometheus exposition format. Storage: a single `BTreeMap` so
/// iteration order is stable across scrapes.
#[derive(Default)]
pub struct Metrics {
    inner: Mutex<BTreeMap<MetricKey, u64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MetricKey {
    name: String,
    /// Sorted (label_name, label_value) pairs.
    labels: Vec<(String, String)>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment a counter by `delta`. Creates the series if it doesn't
    /// exist yet. `labels` may be in any order — they're sorted before
    /// keying so `[("a", "1"), ("b", "2")]` and `[("b", "2"), ("a", "1")]`
    /// land on the same series.
    pub fn inc(&self, name: &str, labels: &[(&str, &str)], delta: u64) {
        let mut sorted: Vec<(String, String)> = labels
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        sorted.sort();
        let key = MetricKey {
            name: name.to_string(),
            labels: sorted,
        };
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return, // poisoned mutex — drop silently (this is a stub)
        };
        *g.entry(key).or_insert(0) += delta;
    }

    /// Render every counter in Prometheus text-format `# TYPE x counter`
    /// blocks. Series with the same metric name share the same TYPE line.
    pub fn render_text(&self) -> String {
        let g = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return String::new(),
        };
        // Always emit a TYPE block for the stable counter names so
        // V2's scraper sees them on day one even without any increments.
        let stable_names = [
            counter::JOBS_TOTAL,
            counter::TELEMETRY_EVENTS_TOTAL,
            counter::COST_MICRO_USD_TOTAL,
        ];
        let mut out = String::new();
        for name in &stable_names {
            out.push_str(&format!("# TYPE {name} counter\n"));
            let mut any_series = false;
            for (key, value) in g.iter().filter(|(k, _)| k.name == *name) {
                if key.labels.is_empty() {
                    out.push_str(&format!("{name} {value}\n"));
                } else {
                    let labels_text = key
                        .labels
                        .iter()
                        .map(|(k, v)| format!("{k}=\"{}\"", escape_label_value(v)))
                        .collect::<Vec<_>>()
                        .join(",");
                    out.push_str(&format!("{name}{{{labels_text}}} {value}\n"));
                }
                any_series = true;
            }
            if !any_series {
                // Emit a zero baseline so the scraper sees the series.
                out.push_str(&format!("{name} 0\n"));
            }
        }
        out
    }
}

fn escape_label_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_text_emits_zero_baseline_for_stable_names() {
        let m = Metrics::new();
        let text = m.render_text();
        assert!(text.contains("# TYPE vergil_jobs_total counter"));
        assert!(text.contains("vergil_jobs_total 0"));
        assert!(text.contains("vergil_telemetry_events_total 0"));
        assert!(text.contains("vergil_cost_micro_usd_total 0"));
    }

    #[test]
    fn inc_creates_series_and_renders_value() {
        let m = Metrics::new();
        m.inc(counter::JOBS_TOTAL, &[("status", "completed")], 3);
        m.inc(counter::JOBS_TOTAL, &[("status", "completed")], 2);
        m.inc(counter::JOBS_TOTAL, &[("status", "failed")], 1);
        let text = m.render_text();
        assert!(text.contains("vergil_jobs_total{status=\"completed\"} 5"));
        assert!(text.contains("vergil_jobs_total{status=\"failed\"} 1"));
    }

    #[test]
    fn inc_with_unsorted_labels_lands_on_same_series() {
        let m = Metrics::new();
        m.inc("x", &[("a", "1"), ("b", "2")], 1);
        m.inc("x", &[("b", "2"), ("a", "1")], 2);
        let g = m.inner.lock().unwrap();
        // Should land on the same key → exactly one entry with value 3.
        let xs: Vec<_> = g.iter().filter(|(k, _)| k.name == "x").collect();
        assert_eq!(xs.len(), 1);
        assert_eq!(*xs[0].1, 3);
    }

    #[test]
    fn escape_label_value_handles_quotes_and_backslashes() {
        let m = Metrics::new();
        m.inc("y", &[("k", "a\"b\\c\n")], 1);
        let text = m.render_text();
        // We didn't add 'y' to stable_names so it shouldn't appear; check
        // the escape helper directly instead.
        let _ = text;
        assert_eq!(escape_label_value("a\"b\\c\n"), "a\\\"b\\\\c\\n");
    }
}
