use axum::Json;
use axum::extract::State;
use sea_orm::*;
use serde::Serialize;
use tracing::instrument;

use crate::entity::test_case;
use crate::error::{AppError, ErrorBody};
use crate::extractors::auth::AuthUser;
use crate::extractors::path::AppPath;
use crate::state::AppState;
use crate::utils::contest::{
    check_contest_access, find_contest, find_contest_problem, require_contest_started,
};
use crate::utils::test_case_body::read_test_case_body;

#[derive(Serialize, utoipa::ToSchema)]
pub struct ProblemSamplesResponse {
    pub samples: Vec<SampleCase>,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct SampleCase {
    pub input: String,
    pub output: String,
    /// Optional markdown note explaining this sample (author-written).
    pub description: Option<String>,
}
#[instrument(skip(state, auth_user), fields(contest_id, problem_id))]
#[utoipa::path(
    get,
    path = "/{problem_id}/samples",
    tag = "Contest Problems",
    operation_id = "getContestProblemSamples",
    summary = "Get sample test cases for a contest problem",
    description = "Returns sample test cases (input and output) for a problem within a contest. Requires the user to be a participant or have contest:manage permission. The contest must have started.",
    params(
        ("id" = i32, Path, description = "Contest ID"),
        ("problem_id" = i32, Path, description = "Problem ID"),
    ),
    responses(
        (status = 200, description = "Sample test cases", body = ProblemSamplesResponse),
        (status = 401, description = "Unauthorized (TOKEN_MISSING, TOKEN_INVALID)", body = ErrorBody),
        (status = 403, description = "Forbidden (PERMISSION_DENIED)", body = ErrorBody),
        (status = 404, description = "Contest or problem not found (NOT_FOUND)", body = ErrorBody),
    ),
    security(("jwt" = [])),
)]
pub async fn get_contest_problem_samples(
    auth_user: AuthUser,
    State(state): State<AppState>,
    AppPath((contest_id, problem_id)): AppPath<(i32, i32)>,
) -> Result<Json<ProblemSamplesResponse>, AppError> {
    let contest_model = find_contest(&state.db, contest_id).await?;
    check_contest_access(&state.db, &auth_user, &contest_model).await?;
    require_contest_started(&auth_user, &contest_model)?;

    let _cp = find_contest_problem(&state.db, contest_id, problem_id).await?;

    let sample_test_cases = test_case::Entity::find()
        .filter(test_case::Column::ProblemId.eq(problem_id))
        .filter(test_case::Column::IsSample.eq(true))
        .order_by_asc(test_case::Column::Position)
        .all(&state.db)
        .await?;

    let mut samples = Vec::with_capacity(sample_test_cases.len());
    for tc in sample_test_cases {
        let input =
            read_test_case_body(&tc.input, tc.input_blob_hash.as_deref(), &*state.blob_store)
                .await?;
        let output = read_test_case_body(
            &tc.expected_output,
            tc.expected_output_blob_hash.as_deref(),
            &*state.blob_store,
        )
        .await?;
        samples.push(SampleCase {
            input,
            output,
            description: tc.description,
        });
    }

    Ok(Json(ProblemSamplesResponse { samples }))
}
