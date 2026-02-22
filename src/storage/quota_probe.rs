//! Probe Anthropic API to read unified rate-limit headers.
//!
//! Sends a minimal Haiku request (1 token) and parses the response headers
//! for 5h/7d utilization data.  Runs in a separate thread, never blocks
//! the heartbeat loop.

use std::path::Path;
use std::time::Duration;

use super::beat::BeatState;
use super::credentials;

/// Minimum interval between probes (seconds).
const PROBE_INTERVAL_SECS: i64 = 900; // 15 minutes

/// HTTP timeout for the probe request.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Result of a successful probe.
#[derive(Debug, Clone)]
pub struct QuotaSnapshot {
    pub utilization_5h: f64,
    pub utilization_7d: f64,
    pub status_5h: String,
    pub status_7d: String,
    pub representative_claim: String,
    pub reset_5h: Option<u64>,
    pub reset_7d: Option<u64>,
}

/// Check if enough time has elapsed since last probe.
pub fn should_probe(beat: &BeatState) -> bool {
    let last = match &beat.quota_updated_at {
        Some(ts) => match ts.parse::<chrono::DateTime<chrono::Utc>>() {
            Ok(t) => t,
            Err(_) => return true,
        },
        None => return true,
    };
    let elapsed = (chrono::Utc::now() - last).num_seconds();
    elapsed >= PROBE_INTERVAL_SECS
}

/// Spawn a background thread that probes the API and updates beat.json.
/// Returns immediately — never blocks the caller.
pub fn spawn_probe(project_hash: String, agent_id: String) {
    std::thread::Builder::new()
        .name("quota-probe".into())
        .spawn(move || {
            if let Err(e) = run_probe(&project_hash, &agent_id) {
                tracing::warn!(error = %e, "Quota probe failed");
            }
        })
        .ok();
}

fn run_probe(project_hash: &str, agent_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Read OAuth token (checks expiresAt)
    let token = credentials::read_oauth_token()
        .ok_or("No valid OAuth token (expired or absent)")?;

    // 2. Detect plan (for plan_type / plan_tier fields)
    let plan = credentials::detect_plan();

    // 3. Send minimal Haiku request
    let body = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "."}]
    });

    let response = ureq::post("https://api.anthropic.com/v1/messages")
        .header("Authorization", &format!("Bearer {}", token))
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .config()
        .timeout_global(Some(PROBE_TIMEOUT))
        .build()
        .send(serde_json::to_vec(&body)?.as_slice())?;

    // 4. Parse unified rate-limit headers
    let headers = response.headers();
    let h = |name: &str| -> Option<String> {
        headers.get(name).and_then(|v| v.to_str().ok()).map(|v| v.to_string())
    };
    let hf = |name: &str| -> Option<f64> {
        headers.get(name).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok())
    };
    let hu = |name: &str| -> Option<u64> {
        headers.get(name).and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok())
    };

    let snapshot = QuotaSnapshot {
        utilization_5h: hf("anthropic-ratelimit-unified-5h-utilization").unwrap_or(0.0),
        utilization_7d: hf("anthropic-ratelimit-unified-7d-utilization").unwrap_or(0.0),
        status_5h: h("anthropic-ratelimit-unified-5h-status").unwrap_or_default(),
        status_7d: h("anthropic-ratelimit-unified-7d-status").unwrap_or_default(),
        representative_claim: h("anthropic-ratelimit-unified-representative-claim").unwrap_or_default(),
        reset_5h: hu("anthropic-ratelimit-unified-5h-reset"),
        reset_7d: hu("anthropic-ratelimit-unified-7d-reset"),
    };

    tracing::info!(
        u5h = snapshot.utilization_5h,
        u7d = snapshot.utilization_7d,
        claim = %snapshot.representative_claim,
        "Quota probe successful"
    );

    // 5. Update beat.json
    let data_dir = super::path_utils::agent_data_dir(project_hash, agent_id);
    update_beat(&data_dir, &snapshot, plan.as_ref());

    Ok(())
}

fn update_beat(data_dir: &Path, snap: &QuotaSnapshot, plan: Option<&credentials::PlanInfo>) {
    let mut beat = BeatState::load(data_dir);

    if let Some(p) = plan {
        beat.plan_type = Some(p.subscription_type.clone());
        beat.plan_tier = Some(p.rate_limit_tier.clone());
    }

    beat.quota_5h = Some(snap.utilization_5h);
    beat.quota_7d = Some(snap.utilization_7d);
    beat.quota_status_5h = Some(snap.status_5h.clone());
    beat.quota_status_7d = Some(snap.status_7d.clone());
    beat.quota_constraint = Some(snap.representative_claim.clone());
    beat.quota_reset_5h = snap.reset_5h;
    beat.quota_reset_7d = snap.reset_7d;
    beat.quota_updated_at = Some(chrono::Utc::now().to_rfc3339());

    // Phase 3: Alerts
    let alert = if snap.utilization_7d > 0.90 {
        tracing::warn!(u7d = snap.utilization_7d, "7-day quota above 90%");
        Some("7d_critical".to_string())
    } else if snap.utilization_5h > 0.80 {
        tracing::warn!(u5h = snap.utilization_5h, "5-hour quota above 80%");
        Some("5h_warning".to_string())
    } else {
        None
    };
    beat.quota_alert = alert;

    beat.save(data_dir);
}
