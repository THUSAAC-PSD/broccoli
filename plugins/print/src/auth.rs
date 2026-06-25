//! Station auth. Station routes carry no `permission`, so the proxy passes them
//! through and we validate the `PrintStation <token>` header ourselves.

use broccoli_server_sdk::Host;
use broccoli_server_sdk::prelude::*;

use crate::config;

const SCHEME: &str = "PrintStation ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StationAuth {
    /// None grants every contest. Some(id) restricts to one contest.
    pub contest_filter: Option<i32>,
}

pub fn extract_token(req: &PluginHttpRequest) -> Option<String> {
    let value = req.headers.get("authorization")?;
    let token = value.strip_prefix(SCHEME)?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

/// Constant-time compare so a token can't be recovered through timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Never short-circuits, so the matching position stays unobservable.
fn token_in(token: &str, allowed: &[String]) -> bool {
    let mut found = false;
    for t in allowed {
        found |= constant_time_eq(t.as_bytes(), token.as_bytes());
    }
    found
}

/// A global token passes outright. Otherwise the request must name a
/// `contest_id` whose tokens include the presented one.
pub fn authenticate_station(host: &Host, req: &PluginHttpRequest) -> Result<StationAuth, ApiError> {
    let token = extract_token(req)
        .ok_or_else(|| PluginHttpResponse::error(401, "Missing print-station token"))?;

    let global = config::load_global_config(host);
    if token_in(&token, &global.station_tokens) {
        return Ok(StationAuth {
            contest_filter: None,
        });
    }

    if let Some(contest_id) = req
        .query
        .get("contest_id")
        .and_then(|s| s.parse::<i32>().ok())
    {
        let contest = config::load_contest_config(host, contest_id);
        if token_in(&token, &contest.station_tokens) {
            return Ok(StationAuth {
                contest_filter: Some(contest_id),
            });
        }
    }

    Err(PluginHttpResponse::error(403, "Invalid print-station token").into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn req_with_auth(value: Option<&str>, contest_id: Option<&str>) -> PluginHttpRequest {
        let mut headers = HashMap::new();
        if let Some(v) = value {
            headers.insert("authorization".to_string(), v.to_string());
        }
        let mut query = HashMap::new();
        if let Some(c) = contest_id {
            query.insert("contest_id".to_string(), c.to_string());
        }
        PluginHttpRequest {
            method: "GET".into(),
            path: String::new(),
            params: HashMap::new(),
            query,
            headers,
            body: None,
            auth: None,
        }
    }

    #[test]
    fn extracts_print_station_token() {
        let req = req_with_auth(Some("PrintStation secret-123"), None);
        assert_eq!(extract_token(&req).as_deref(), Some("secret-123"));
    }

    #[test]
    fn ignores_bearer_tokens() {
        let req = req_with_auth(Some("Bearer jwt.here"), None);
        assert_eq!(extract_token(&req), None);
    }

    #[test]
    fn global_token_grants_unrestricted_access() {
        let host = Host::mock();
        host.config.seed(
            "plugin",
            "",
            config::NAMESPACE,
            json!({ "station_tokens": ["G"] }),
        );
        let req = req_with_auth(Some("PrintStation G"), None);
        let auth = authenticate_station(&host, &req).unwrap();
        assert_eq!(auth.contest_filter, None);
    }

    #[test]
    fn contest_token_is_restricted_to_that_contest() {
        let host = Host::mock();
        host.config.seed(
            "contest",
            "7",
            config::NAMESPACE,
            json!({ "station_tokens": ["C7"] }),
        );
        let req = req_with_auth(Some("PrintStation C7"), Some("7"));
        let auth = authenticate_station(&host, &req).unwrap();
        assert_eq!(auth.contest_filter, Some(7));
    }

    #[test]
    fn constant_time_eq_matches_only_identical_bytes() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreX"));
        assert!(!constant_time_eq(b"secret", b"secre")); // length mismatch
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn token_in_finds_match_anywhere_in_list() {
        let allowed = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(token_in("a", &allowed));
        assert!(token_in("c", &allowed));
        assert!(!token_in("d", &allowed));
        assert!(!token_in("", &allowed));
    }

    #[test]
    fn unknown_token_is_rejected() {
        let host = Host::mock();
        host.config.seed(
            "plugin",
            "",
            config::NAMESPACE,
            json!({ "station_tokens": ["G"] }),
        );
        let req = req_with_auth(Some("PrintStation nope"), Some("7"));
        let err = authenticate_station(&host, &req).unwrap_err();
        assert_eq!(err.into_response().status, 403);
    }

    #[test]
    fn missing_token_is_unauthorized() {
        let host = Host::mock();
        let req = req_with_auth(None, None);
        let err = authenticate_station(&host, &req).unwrap_err();
        assert_eq!(err.into_response().status, 401);
    }
}
