//! Shared helpers for contest-type plugin API handlers.
//!
//! Provides contest metadata loading, access control, and config loading
//! that every contest plugin (ICPC, IOI, etc.) needs.

use serde::Deserialize;

use crate::api::ApiError;
#[cfg(feature = "guest")]
use crate::error::SdkError;
#[cfg(feature = "guest")]
use crate::types::PluginHttpRequest;
use crate::types::PluginHttpResponse;

/// Contest metadata loaded from the database, used for route-level access control.
#[derive(Debug, Clone, Deserialize)]
pub struct ContestInfo {
    pub contest_type: Option<String>,
    pub is_public: bool,
    pub is_active: bool,
    /// `"before"`, `"during"`, or `"after"`.
    pub phase: String,
}

impl ContestInfo {
    /// Verify this contest matches the expected `contest_type`.
    /// Returns a 404 `ApiError` if not.
    pub fn require_type(&self, expected: &str) -> Result<(), ApiError> {
        if self.contest_type.as_deref() == Some(expected) {
            Ok(())
        } else {
            Err(PluginHttpResponse::error(404, format!("Not a {expected} contest")).into())
        }
    }
}

/// Load contest routing metadata (type, visibility, phase).
///
/// Returns 404 `ApiError` if the contest does not exist.
#[cfg(feature = "guest")]
pub fn load_info(host: &crate::sdk::Host, contest_id: i32) -> Result<ContestInfo, ApiError> {
    let mut p = crate::db::Params::new();
    let sql = format!(
        "SELECT contest_type, is_public, \
            ((activate_time IS NULL OR activate_time <= NOW()) AND \
             (deactivate_time IS NULL OR deactivate_time > NOW())) AS is_active, \
            CASE \
                WHEN NOW() < start_time THEN 'before' \
                WHEN NOW() > end_time THEN 'after' \
                ELSE 'during' \
            END AS phase \
         FROM contest WHERE id = {}",
        p.bind(contest_id)
    );
    host.db
        .query_one_with_args::<ContestInfo>(&sql, &p.into_args())
        .map_err(ApiError::from)?
        .ok_or_else(|| PluginHttpResponse::error(404, "Contest not found").into())
}

/// Load contest metadata and verify the requesting user can view it.
///
/// Returns 404 `ApiError` if the contest does not exist or the user lacks access
/// (404 instead of 403 to prevent enumeration).
#[cfg(feature = "guest")]
pub fn check_access(
    host: &crate::sdk::Host,
    req: &PluginHttpRequest,
    contest_id: i32,
) -> Result<ContestInfo, ApiError> {
    let info = load_info(host, contest_id)?;
    if !can_view(host, req, contest_id, &info)? {
        return Err(PluginHttpResponse::error(404, "Contest not found").into());
    }
    Ok(info)
}

/// Check if the requesting user can view a contest.
///
/// Admins can always view. Otherwise requires active + (public OR participant).
#[cfg(feature = "guest")]
fn can_view(
    host: &crate::sdk::Host,
    req: &PluginHttpRequest,
    contest_id: i32,
    info: &ContestInfo,
) -> Result<bool, ApiError> {
    if req.has_permission("contest:manage") {
        return Ok(true);
    }
    if !info.is_active {
        return Ok(false);
    }
    if info.is_public {
        return Ok(true);
    }
    match req.user_id() {
        Some(user_id) => is_participant(host, contest_id, user_id).map_err(ApiError::from),
        None => Ok(false),
    }
}

/// Check if a user is enrolled as a participant in a contest.
#[cfg(feature = "guest")]
pub fn is_participant(
    host: &crate::sdk::Host,
    contest_id: i32,
    user_id: i32,
) -> Result<bool, SdkError> {
    #[derive(Deserialize)]
    struct ExistsRow {
        exists: bool,
    }

    let mut p = crate::db::Params::new();
    let sql = format!(
        "SELECT EXISTS( \
            SELECT 1 FROM contest_user \
            WHERE contest_id = {} AND user_id = {} \
         ) AS exists",
        p.bind(contest_id),
        p.bind(user_id)
    );
    Ok(host
        .db
        .query_one_with_args::<ExistsRow>(&sql, &p.into_args())?
        .is_some_and(|row| row.exists))
}

/// Check if a problem belongs to a contest.
#[cfg(feature = "guest")]
pub fn has_problem(
    host: &crate::sdk::Host,
    contest_id: i32,
    problem_id: i32,
) -> Result<bool, SdkError> {
    #[derive(Deserialize)]
    struct ExistsRow {
        exists: bool,
    }

    let mut p = crate::db::Params::new();
    let sql = format!(
        "SELECT EXISTS( \
            SELECT 1 FROM contest_problem \
            WHERE contest_id = {} AND problem_id = {} \
         ) AS exists",
        p.bind(contest_id),
        p.bind(problem_id)
    );
    Ok(host
        .db
        .query_one_with_args::<ExistsRow>(&sql, &p.into_args())?
        .is_some_and(|row| row.exists))
}

/// Verify a user can access a specific problem within a contest.
///
/// Requires: contest exists, user is admin or active participant, problem belongs to contest.
/// Returns 404 for all denial cases to prevent enumeration.
///
/// This is for resource plugins (cooldown, submission-limit) that check access at the
/// problem level within a contest, unlike `check_access` which checks contest-level visibility.
#[cfg(feature = "guest")]
pub fn check_problem_access(
    host: &crate::sdk::Host,
    req: &PluginHttpRequest,
    contest_id: i32,
    user_id: i32,
    problem_id: i32,
) -> Result<(), ApiError> {
    let info = load_info(host, contest_id)?;

    if !req.has_permission("contest:manage")
        && (!info.is_active || !is_participant(host, contest_id, user_id)?)
    {
        return Err(PluginHttpResponse::error(404, "Contest not found").into());
    }

    if !has_problem(host, contest_id, problem_id)? {
        return Err(PluginHttpResponse::error(404, "Contest not found").into());
    }

    Ok(())
}

/// Load a contest-scoped plugin config, deserializing into `T`.
///
/// Returns `T::default()` if no config has been set for this contest.
#[cfg(feature = "guest")]
pub fn load_config<T: serde::de::DeserializeOwned + Default>(
    host: &crate::sdk::Host,
    contest_id: i32,
) -> Result<T, SdkError> {
    Ok(
        serde_json::from_value(host.config.get_contest(contest_id, "contest")?.config)
            .unwrap_or_default(),
    )
}
