//! Provider-level failover: walk the enabled provider list top to bottom
//! when the current provider returns an upstream error. Quota-exhausted
//! providers are skipped automatically.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::providers::{ProviderKind, ProviderStore, OFFICIAL_PROVIDER_ID};

/// Consecutive upstream failures before a provider entry is skipped.
pub const FAILOVER_FAILURE_THRESHOLD: u32 = 3;
/// How long a tripped provider entry is skipped before requests probe it again.
pub const FAILOVER_COOLDOWN: Duration = Duration::from_secs(30);
/// How long a quota-exhausted mark persists before the provider is probed again.
pub const QUOTA_EXHAUSTED_COOLDOWN: Duration = Duration::from_secs(300);

/// HTTP statuses that mark an upstream attempt as failed and trigger
/// failover. Other 4xx statuses describe the request itself, so they are
/// forwarded to Codex unchanged.
pub fn is_failover_status(status: u16) -> bool {
    matches!(status, 401 | 403 | 404 | 408 | 429) || status >= 500
}

/// HTTP status that indicates quota exhaustion (rate or plan limit hit).
pub fn is_quota_exhausted_status(status: u16) -> bool {
    status == 429
}

/// One upstream stop in the failover walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailoverCandidate {
    pub provider_id: String,
    pub model: String,
}

/// In-memory circuit-breaker state for failover entries. Helper restarts
/// clear it, so a stale unhealthy mark never outlives the process.
#[derive(Default)]
pub struct FailoverHealth {
    entries: Mutex<HashMap<String, HealthEntry>>,
}

#[derive(Debug, Clone, Copy)]
struct HealthEntry {
    consecutive_failures: u32,
    marked_unhealthy_at: Option<Instant>,
}

fn health_key(provider_id: &str, model: &str) -> String {
    format!(
        "{}\u{0}{}",
        provider_id.trim().to_ascii_lowercase(),
        model.trim().to_ascii_lowercase()
    )
}

impl FailoverHealth {
    pub fn new() -> Self {
        Self::default()
    }

    /// A tripped entry recovers automatically once `FAILOVER_COOLDOWN` has
    /// elapsed; the next request then acts as the probe. A failed probe
    /// refreshes the cooldown, a successful one clears the entry.
    pub fn is_available(&self, provider_id: &str, model: &str) -> bool {
        let entries = self.entries.lock().expect("failover health lock");
        let Some(entry) = entries.get(&health_key(provider_id, model)) else {
            return true;
        };
        if entry.consecutive_failures < FAILOVER_FAILURE_THRESHOLD {
            return true;
        }
        match entry.marked_unhealthy_at {
            Some(marked_at) => marked_at.elapsed() >= FAILOVER_COOLDOWN,
            None => true,
        }
    }

    pub fn record_failure(&self, provider_id: &str, model: &str) {
        let mut entries = self.entries.lock().expect("failover health lock");
        let entry = entries
            .entry(health_key(provider_id, model))
            .or_insert(HealthEntry {
                consecutive_failures: 0,
                marked_unhealthy_at: None,
            });
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        if entry.consecutive_failures >= FAILOVER_FAILURE_THRESHOLD {
            entry.marked_unhealthy_at = Some(Instant::now());
        }
    }

    pub fn record_success(&self, provider_id: &str, model: &str) {
        let mut entries = self.entries.lock().expect("failover health lock");
        entries.remove(&health_key(provider_id, model));
    }
}

/// Shared quota-exhaustion tracker. Providers marked here are skipped in
/// failover walks and their usage pie turns red in the settings UI.
static QUOTA_STATE: OnceLock<QuotaState> = OnceLock::new();

pub struct QuotaState {
    exhausted: Mutex<HashMap<String, Instant>>,
}

impl QuotaState {
    pub fn global() -> &'static QuotaState {
        QUOTA_STATE.get_or_init(|| QuotaState {
            exhausted: Mutex::new(HashMap::new()),
        })
    }

    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            exhausted: Mutex::new(HashMap::new()),
        }
    }

    pub fn mark_exhausted(&self, provider_id: &str) {
        let mut map = self.exhausted.lock().expect("quota state lock");
        map.insert(
            provider_id.trim().to_ascii_lowercase(),
            Instant::now(),
        );
    }

    /// Called from usage queries. `percent >= 100` marks exhausted,
    /// anything below clears the mark.
    pub fn record_percent(&self, provider_id: &str, percent: f64) {
        if percent >= 100.0 {
            self.mark_exhausted(provider_id);
        } else {
            self.clear(provider_id);
        }
    }

    pub fn clear(&self, provider_id: &str) {
        let mut map = self.exhausted.lock().expect("quota state lock");
        map.remove(&provider_id.trim().to_ascii_lowercase());
    }

    /// Returns true if the provider was marked exhausted within the
    /// `QUOTA_EXHAUSTED_COOLDOWN` window.
    pub fn is_exhausted(&self, provider_id: &str) -> bool {
        let map = self.exhausted.lock().expect("quota state lock");
        match map.get(&provider_id.trim().to_ascii_lowercase()) {
            Some(marked_at) => marked_at.elapsed() < QUOTA_EXHAUSTED_COOLDOWN,
            None => false,
        }
    }
}

/// Candidate order for a request: the primary provider first, then the
/// remaining enabled providers in `selected_ids` order using their default
/// `model`. Official / OAuth providers and providers without a default
/// model are skipped. Duplicates of the primary are dropped.
pub fn failover_candidates(
    store: &ProviderStore,
    primary_provider_id: &str,
) -> Vec<FailoverCandidate> {
    let mut candidates = Vec::new();
    let primary = primary_provider_id.trim();
    if !primary.is_empty() {
        if let Some(provider) = store.providers.iter().find(|p| p.id == primary) {
            if provider.kind == ProviderKind::ApiKey && !provider.model.trim().is_empty() {
                candidates.push(FailoverCandidate {
                    provider_id: provider.id.clone(),
                    model: provider.model.trim().to_string(),
                });
            }
        }
    }
    for provider in crate::providers::selected_api_providers(store) {
        if provider.id.eq_ignore_ascii_case(primary) {
            continue;
        }
        if provider.id == OFFICIAL_PROVIDER_ID || provider.kind != ProviderKind::ApiKey {
            continue;
        }
        let model = provider.model.trim();
        if model.is_empty() {
            continue;
        }
        if candidates
            .iter()
            .any(|c| c.provider_id.eq_ignore_ascii_case(&provider.id))
        {
            continue;
        }
        candidates.push(FailoverCandidate {
            provider_id: provider.id.clone(),
            model: model.to_string(),
        });
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{Provider, ProviderStore};

    fn provider(id: &str, model: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: id.to_string(),
            kind: ProviderKind::ApiKey,
            model: model.to_string(),
            ..Provider::default()
        }
    }

    fn store(ids: &[&str], models: &[&str]) -> ProviderStore {
        let providers: Vec<Provider> = ids
            .iter()
            .zip(models.iter())
            .map(|(id, model)| provider(id, model))
            .collect();
        ProviderStore {
            active_id: ids.first().unwrap().to_string(),
            selected_ids: ids.iter().map(|id| id.to_string()).collect(),
            providers,
        }
    }

    #[test]
    fn failover_status_codes() {
        assert!(is_failover_status(401));
        assert!(is_failover_status(403));
        assert!(is_failover_status(404));
        assert!(is_failover_status(408));
        assert!(is_failover_status(429));
        assert!(is_failover_status(500));
        assert!(is_failover_status(503));
        assert!(!is_failover_status(400));
        assert!(!is_failover_status(422));
        assert!(!is_failover_status(200));
    }

    #[test]
    fn quota_exhausted_status() {
        assert!(is_quota_exhausted_status(429));
        assert!(!is_quota_exhausted_status(500));
    }

    #[test]
    fn candidates_follow_selected_order() {
        let store = store(&["a", "b", "c"], &["model-a", "model-b", "model-c"]);
        let candidates = failover_candidates(&store, "a");
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].provider_id, "a");
        assert_eq!(candidates[1].provider_id, "b");
        assert_eq!(candidates[2].provider_id, "c");
        assert_eq!(candidates[1].model, "model-b");
    }

    #[test]
    fn candidates_skip_primary_duplicate() {
        let store = store(&["a", "b"], &["model-a", "model-b"]);
        let candidates = failover_candidates(&store, "b");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].provider_id, "b");
        assert_eq!(candidates[1].provider_id, "a");
    }

    #[test]
    fn candidates_skip_empty_model() {
        let store = store(&["a", "b", "c"], &["model-a", "", "model-c"]);
        let candidates = failover_candidates(&store, "a");
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].provider_id, "a");
        assert_eq!(candidates[1].provider_id, "c");
    }

    #[test]
    fn candidates_skip_official() {
        let mut s = store(&["a", "b"], &["model-a", "model-b"]);
        s.providers.push(Provider {
            id: OFFICIAL_PROVIDER_ID.to_string(),
            name: "Official".to_string(),
            kind: ProviderKind::Oauth,
            ..Provider::default()
        });
        s.selected_ids.push(OFFICIAL_PROVIDER_ID.to_string());
        let candidates = failover_candidates(&s, "a");
        assert_eq!(candidates.len(), 2);
        assert!(!candidates.iter().any(|c| c.provider_id == OFFICIAL_PROVIDER_ID));
    }

    #[test]
    fn health_trips_after_threshold() {
        let health = FailoverHealth::new();
        let key_provider = "test-provider";
        let key_model = "test-model";
        assert!(health.is_available(key_provider, key_model));
        health.record_failure(key_provider, key_model);
        health.record_failure(key_provider, key_model);
        assert!(health.is_available(key_provider, key_model));
        health.record_failure(key_provider, key_model);
        assert!(!health.is_available(key_provider, key_model));
        health.record_success(key_provider, key_model);
        assert!(health.is_available(key_provider, key_model));
    }

    #[test]
    fn quota_state_marks_and_clears() {
        let state = QuotaState::new();
        assert!(!state.is_exhausted("prov"));
        state.mark_exhausted("prov");
        assert!(state.is_exhausted("prov"));
        state.clear("prov");
        assert!(!state.is_exhausted("prov"));
    }

    #[test]
    fn quota_state_record_percent() {
        let state = QuotaState::new();
        state.record_percent("prov", 99.5);
        assert!(!state.is_exhausted("prov"));
        state.record_percent("prov", 100.0);
        assert!(state.is_exhausted("prov"));
        state.record_percent("prov", 50.0);
        assert!(!state.is_exhausted("prov"));
    }

    #[test]
    fn quota_state_case_insensitive() {
        let state = QuotaState::new();
        state.mark_exhausted("MyProvider");
        assert!(state.is_exhausted("myprovider"));
    }
}

static FAILOVER_HEALTH: OnceLock<FailoverHealth> = OnceLock::new();

impl FailoverHealth {
    pub fn global() -> &'static FailoverHealth {
        FAILOVER_HEALTH.get_or_init(FailoverHealth::new)
    }
}
