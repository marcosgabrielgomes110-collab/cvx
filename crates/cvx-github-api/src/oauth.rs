use std::time::Duration;

use cvx_core::{Error, Result};
use serde::Deserialize;
use serde_json::Value;

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Device-flow authorization start response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    /// Opaque code used while polling.
    pub device_code: String,
    /// Short code shown to the user.
    pub user_code: String,
    /// Verification page.
    pub verification_uri: String,
    /// Lifetime in seconds.
    pub expires_in: u64,
    /// Minimum polling interval in seconds.
    pub interval: u64,
}

/// One device-flow poll result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevicePoll {
    /// The user has not authorized yet.
    Pending,
    /// Poll less frequently.
    SlowDown,
    /// Authorization succeeded.
    Authorized(OAuthToken),
    /// The user denied the request.
    AccessDenied,
    /// The device code expired.
    Expired,
}

/// OAuth token returned by the device flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthToken {
    /// Access token.
    pub access_token: String,
    /// Token type, normally `bearer`.
    pub token_type: String,
    /// Granted scopes.
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Starts a GitHub OAuth device flow.
pub fn request_device_code(client_id: &str, scopes: &[String]) -> Result<DeviceCode> {
    validate_client_id(client_id)?;
    validate_scopes(scopes)?;

    let response = agent()
        .post(DEVICE_CODE_URL)
        .set("Accept", "application/json")
        .send_form(&[("client_id", client_id), ("scope", &scopes.join(" "))])
        .map_err(|error| oauth_error("requesting a device code", error))?;
    let text = response
        .into_string()
        .map_err(|error| Error::corrupt(format!("invalid device-code response: {error}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| Error::corrupt(format!("invalid device-code response: {error}")))?;
    parse_device_code(&value)
}

/// Polls the token endpoint once.
pub fn poll_device_token(client_id: &str, device_code: &str) -> Result<DevicePoll> {
    validate_client_id(client_id)?;
    if device_code.is_empty() {
        return Err(Error::Message("device code is empty".to_owned()));
    }

    let response = agent()
        .post(TOKEN_URL)
        .set("Accept", "application/json")
        .send_form(&[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", DEVICE_GRANT),
        ])
        .map_err(|error| oauth_error("polling the device token", error))?;
    let text = response
        .into_string()
        .map_err(|error| Error::corrupt(format!("invalid token response: {error}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| Error::corrupt(format!("invalid token response: {error}")))?;
    parse_token_poll(&value)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("cvx/", env!("CARGO_PKG_VERSION")))
        .build()
}

fn parse_device_code(value: &Value) -> Result<DeviceCode> {
    let response: DeviceCodeResponse = serde_json::from_value(value.clone())
        .map_err(|error| Error::corrupt(format!("invalid device-code response: {error}")))?;
    if response.device_code.is_empty() || response.user_code.is_empty() {
        return Err(Error::corrupt(
            "device-code response is missing required fields",
        ));
    }
    Ok(DeviceCode {
        device_code: response.device_code,
        user_code: response.user_code,
        verification_uri: response.verification_uri,
        expires_in: response.expires_in,
        interval: response.interval.max(1),
    })
}

fn parse_token_poll(value: &Value) -> Result<DevicePoll> {
    let response: TokenResponse = serde_json::from_value(value.clone())
        .map_err(|error| Error::corrupt(format!("invalid token response: {error}")))?;

    if let Some(token) = response.access_token {
        return Ok(DevicePoll::Authorized(OAuthToken {
            access_token: token,
            token_type: response.token_type.unwrap_or_else(|| "bearer".to_owned()),
            scopes: split_scopes(response.scope.as_deref()),
        }));
    }

    match response.error.as_deref() {
        Some("authorization_pending") => Ok(DevicePoll::Pending),
        Some("slow_down") => Ok(DevicePoll::SlowDown),
        Some("access_denied") => Ok(DevicePoll::AccessDenied),
        Some("expired_token") => Ok(DevicePoll::Expired),
        Some(error) => {
            let description = response
                .error_description
                .unwrap_or_else(|| error.to_owned());
            Err(Error::Message(format!("GitHub OAuth error: {description}")))
        }
        None => Err(Error::corrupt(
            "GitHub OAuth response contains neither a token nor an error",
        )),
    }
}

fn oauth_error(context: &str, error: ureq::Error) -> Error {
    match error {
        ureq::Error::Status(code, response) => {
            let text = response.into_string().unwrap_or_default();
            Error::Message(format!(
                "GitHub OAuth failed while {context} (HTTP {code}): {}",
                text.trim()
            ))
        }
        ureq::Error::Transport(transport) => Error::Message(format!(
            "GitHub OAuth transport error while {context}: {transport}"
        )),
    }
}

fn validate_client_id(client_id: &str) -> Result<()> {
    if client_id.trim().is_empty() {
        return Err(Error::Message(
            "GitHub OAuth client id is required; set CVX_GITHUB_CLIENT_ID or pass --client-id"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_scopes(scopes: &[String]) -> Result<()> {
    for scope in scopes {
        if scope.is_empty()
            || !scope
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
        {
            return Err(Error::Message(format!("invalid OAuth scope '{scope}'")));
        }
    }
    Ok(())
}

fn split_scopes(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .split([',', ' '])
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Builds an OAuth request body for tests and diagnostics.
#[cfg(test)]
fn device_form(client_id: &str, scopes: &[String]) -> Value {
    serde_json::json!({
        "client_id": client_id,
        "scope": scopes.join(" "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_code() {
        let value = serde_json::json!({
            "device_code": "device",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 5
        });
        let code = parse_device_code(&value).expect("device code");
        assert_eq!(code.device_code, "device");
        assert_eq!(code.user_code, "ABCD-EFGH");
        assert_eq!(code.interval, 5);
    }

    #[test]
    fn parses_token_poll_states() {
        assert_eq!(
            parse_token_poll(&serde_json::json!({"error": "authorization_pending"})).unwrap(),
            DevicePoll::Pending
        );
        assert_eq!(
            parse_token_poll(&serde_json::json!({"error": "slow_down"})).unwrap(),
            DevicePoll::SlowDown
        );
        assert_eq!(
            parse_token_poll(&serde_json::json!({"error": "access_denied"})).unwrap(),
            DevicePoll::AccessDenied
        );
        assert_eq!(
            parse_token_poll(&serde_json::json!({"error": "expired_token"})).unwrap(),
            DevicePoll::Expired
        );

        let token = parse_token_poll(&serde_json::json!({
            "access_token": "secret",
            "token_type": "bearer",
            "scope": "repo, read:org,workflow"
        }))
        .unwrap();
        assert_eq!(
            token,
            DevicePoll::Authorized(OAuthToken {
                access_token: "secret".to_owned(),
                token_type: "bearer".to_owned(),
                scopes: vec![
                    "repo".to_owned(),
                    "read:org".to_owned(),
                    "workflow".to_owned()
                ],
            })
        );
    }

    #[test]
    fn validates_scopes_and_client_ids() {
        assert!(validate_scopes(&["repo".to_owned(), "read:org".to_owned()]).is_ok());
        assert!(validate_scopes(&["bad scope".to_owned()]).is_err());
        assert!(validate_client_id("").is_err());
        assert!(validate_client_id("Iv1.example").is_ok());
        assert_eq!(
            device_form("client", &["repo".to_owned(), "workflow".to_owned()]),
            serde_json::json!({"client_id": "client", "scope": "repo workflow"})
        );
    }
}
