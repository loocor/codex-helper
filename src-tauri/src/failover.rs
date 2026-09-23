//! Provider-level failover: when enabled, walk enabled API providers in
//! list order after the current provider fails because it is out of quota
//! or cannot be reached. Each fallback uses that provider's default model.
//! Official login is never a fallback target.

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
    matches!(status, 401 | 402 | 403 | 404 | 408 | 429) || status >= 500
}

/// HTTP status that indicates quota exhaustion (rate or plan limit hit).
pub fn is_quota_exhausted_status(status: u16) -> bool {
    matches!(status, 402 | 429)
}

/// Body text that means the provider refused the call for quota or balance,
/// even when the status is not 429. Matching is intentionally narrow so a
/// normal request error is not treated as exhaustion.
pub fn is_quota_exhausted_excerpt(excerpt: &str) -> bool {
    let lower = excerpt.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "insufficient_quota",
        "insufficient quota",
        "quota_exceeded",
        "quota exceeded",
        "exceeded your current quota",
        "exceeded_current_quota",
        "billing_hard_limit",
        "insufficient_balance",
        "insufficient balance",
        "credit balance is too low",
        "credit balance too low",
        "out of credits",
        "余额不足",
        "额度不足",
        "额度已用完",
        "配额不足",
        "配额已用完",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// Transport failures that mean this provider cannot be reached. Local
/// configuration errors are not included; those should surface as-is.
pub fn is_transport_failover_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "timed out",
        "timeout",
        "connection reset",
        "connection refused",
        "connection aborted",
        "connection closed",
        "broken pipe",
        "network is unreachable",
        "network unreachable",
        "dns error",
        "failed to lookup",
        "nodename nor servname",
        "no such host",
        "tcp connect",
        "tls handshake",
        "certificate",
        "error trying to connect",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
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
        map.insert(provider_id.trim().to_ascii_lowercase(), Instant::now());
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

fn usable_failover_provider(provider: &crate::providers::Provider) -> bool {
    provider.id != OFFICIAL_PROVIDER_ID
        && provider.kind == ProviderKind::ApiKey
        && !provider.model.trim().is_empty()
}

/// Candidate order: the provider that was supposed to serve this request,
/// then enabled providers after it in the settings list, then providers
/// before it. Each stop uses that provider's default model. Official login
/// and providers without a default model are skipped.
pub fn failover_candidates(
    store: &ProviderStore,
    primary_provider_id: &str,
) -> Vec<FailoverCandidate> {
    let selected = crate::providers::effective_selected_ids(store);
    let enabled: Vec<&crate::providers::Provider> =
        crate::providers::providers_in_display_order(store)
            .into_iter()
            .filter(|provider| {
                usable_failover_provider(provider) && selected.iter().any(|id| id == &provider.id)
            })
            .collect();
    let primary = primary_provider_id.trim();
    let mut ordered = Vec::new();
    if let Some(index) = enabled.iter().position(|provider| provider.id == primary) {
        ordered.extend(enabled.iter().skip(index).copied());
        ordered.extend(enabled.iter().take(index).copied());
    } else {
        if let Some(provider) = store
            .providers
            .iter()
            .find(|provider| provider.id == primary)
        {
            if usable_failover_provider(provider) {
                ordered.push(provider);
            }
        }
        ordered.extend(enabled);
    }
    ordered
        .into_iter()
        .map(|provider| FailoverCandidate {
            provider_id: provider.id.clone(),
            model: provider.model.trim().to_string(),
        })
        .collect()
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
        assert!(is_failover_status(402));
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
        assert!(is_quota_exhausted_status(402));
        assert!(!is_quota_exhausted_status(500));
        assert!(!is_quota_exhausted_status(400));
    }

    #[test]
    fn quota_excerpt_is_narrow() {
        assert!(is_quota_exhausted_excerpt(
            "You exceeded your current quota"
        ));
        assert!(is_quota_exhausted_excerpt("error code insufficient_quota"));
        assert!(is_quota_exhausted_excerpt("余额不足"));
        assert!(!is_quota_exhausted_excerpt("model_not_found"));
        assert!(!is_quota_exhausted_excerpt("quota remaining: 40%"));
    }

    #[test]
    fn transport_message_matches_network_failures_only() {
        assert!(is_transport_failover_message(
            "Provider upstream request failed: connection refused"
        ));
        assert!(is_transport_failover_message("dns error: no such host"));
        assert!(is_transport_failover_message("operation timed out"));
        assert!(!is_transport_failover_message(
            "Provider API key is required"
        ));
        assert!(!is_transport_failover_message(
            "Provider request is not valid JSON"
        ));
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
    fn candidates_continue_down_the_list_then_wrap() {
        let store = store(&["a", "b", "c"], &["model-a", "model-b", "model-c"]);
        let candidates = failover_candidates(&store, "b");
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "a"]
        );
        assert_eq!(candidates[1].model, "model-c");
    }

    #[test]
    fn candidates_follow_display_order_not_selection_insertion_order() {
        let mut store = store(&["a", "b", "c"], &["model-a", "model-b", "model-c"]);
        store.selected_ids = vec!["c".to_string(), "a".to_string(), "b".to_string()];
        let candidates = failover_candidates(&store, "a");
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.provider_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
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
        assert!(!candidates
            .iter()
            .any(|c| c.provider_id == OFFICIAL_PROVIDER_ID));
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
