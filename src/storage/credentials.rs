//! Read Claude CLI credentials to detect the user's Anthropic plan.
//!
//! Source: `~/.claude/.credentials.json`

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct PlanInfo {
    pub subscription_type: String,
    pub rate_limit_tier: String,
    pub is_max: bool,
    /// 1 = pro, 5 = MAX 100$, 20 = MAX 200$
    pub multiplier: u8,
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OAuthBlock>,
}

#[derive(Deserialize)]
struct OAuthBlock {
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier")]
    rate_limit_tier: Option<String>,
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<String>,
}

/// Detect the user's Anthropic plan from `~/.claude/.credentials.json`.
/// Returns `None` if file is absent, unreadable, or no OAuth block found.
pub fn detect_plan() -> Option<PlanInfo> {
    let creds_path = dirs::home_dir()?.join(".claude/.credentials.json");
    let content = std::fs::read_to_string(&creds_path).ok()?;
    let creds: Credentials = serde_json::from_str(&content).ok()?;

    let oauth = creds.claude_ai_oauth?;
    let sub_type = oauth.subscription_type?;
    let tier = oauth.rate_limit_tier?;

    let multiplier = if tier.contains("20x") {
        20
    } else if tier.contains("5x") {
        5
    } else {
        1
    };

    Some(PlanInfo {
        is_max: sub_type == "max",
        subscription_type: sub_type,
        rate_limit_tier: tier,
        multiplier,
    })
}

/// Read the OAuth access token and its expiry.
/// Returns `None` if credentials are absent or token is expired.
pub fn read_oauth_token() -> Option<String> {
    let creds_path = dirs::home_dir()?.join(".claude/.credentials.json");
    let content = std::fs::read_to_string(&creds_path).ok()?;
    let creds: Credentials = serde_json::from_str(&content).ok()?;

    let oauth = creds.claude_ai_oauth?;
    let token = oauth.access_token?;
    let expires_at = oauth.expires_at?;

    // Parse expiresAt (ISO 8601) and check it hasn't expired
    let expiry: chrono::DateTime<chrono::Utc> = expires_at.parse().ok()?;
    if chrono::Utc::now() >= expiry {
        tracing::debug!("OAuth token expired at {}", expires_at);
        return None;
    }

    Some(token)
}
