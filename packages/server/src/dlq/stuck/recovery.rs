use chrono::Utc;
use common::{SubmissionDlqErrorCode, SubmissionStatus};
use sea_orm::{ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter};

use crate::entity::{
    code_run, code_run_result,
    judgement_reset::{ClearJudgementColumns, ClearJudgementFields},
    submission, submission_judgement, test_case_result,
};

use super::{STUCK_RECOVERY_STATUSES, StuckRecovery, detector_retry_lease};

pub(super) async fn recover_stuck_submission_without_steal(
    txn: &DatabaseTransaction,
    submission: &submission::Model,
    server_id: &str,
) -> anyhow::Result<StuckRecovery> {
    let new_retry_count = submission.retry_count.saturating_add(1);
    let new_epoch = submission.judge_epoch.saturating_add(1);
    let lease_heartbeat_at = Utc::now();
    let (owner_server_id, lease_heartbeat_at) = detector_retry_lease(server_id, lease_heartbeat_at);

    let affected = submission::Entity::update_many()
        .col_expr(
            submission::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            submission::Column::RetryCount,
            sea_orm::sea_query::Expr::value(new_retry_count).into(),
        )
        .col_expr(
            submission::Column::JudgeEpoch,
            sea_orm::sea_query::Expr::value(new_epoch).into(),
        )
        .col_expr(
            submission::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(owner_server_id.clone()).into(),
        )
        .col_expr(
            submission::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::value(lease_heartbeat_at).into(),
        )
        .clear_judgement_columns()
        .filter(submission::Column::Id.eq(submission.id))
        .filter(submission::Column::JudgeEpoch.eq(submission.judge_epoch))
        .filter(submission::Column::Status.is_in(STUCK_RECOVERY_STATUSES))
        .exec(txn)
        .await?;

    if affected.rows_affected == 0 {
        return Ok(StuckRecovery::Skip);
    }

    open_retry_submission_judgement(
        txn,
        submission,
        new_epoch,
        owner_server_id.clone(),
        lease_heartbeat_at,
    )
    .await?;

    let mut redispatch_model = submission.clone();
    redispatch_model.status = SubmissionStatus::Pending;
    redispatch_model.retry_count = new_retry_count;
    redispatch_model.judge_epoch = new_epoch;
    redispatch_model.owner_server_id = owner_server_id;
    redispatch_model.lease_heartbeat_at = lease_heartbeat_at;
    redispatch_model.clear_judgement_fields();

    Ok(StuckRecovery::RedispatchSubmission {
        model: redispatch_model,
        retry_count: new_retry_count,
    })
}

pub(super) async fn recover_stuck_code_run_without_steal(
    txn: &DatabaseTransaction,
    run: &code_run::Model,
    server_id: &str,
) -> anyhow::Result<StuckRecovery> {
    let new_retry_count = run.retry_count.saturating_add(1);
    let new_epoch = run.judge_epoch.saturating_add(1);
    let lease_heartbeat_at = Utc::now();
    let (owner_server_id, lease_heartbeat_at) = detector_retry_lease(server_id, lease_heartbeat_at);

    code_run_result::Entity::delete_many()
        .filter(code_run_result::Column::CodeRunId.eq(run.id))
        .exec(txn)
        .await?;

    let affected = code_run::Entity::update_many()
        .col_expr(
            code_run::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            code_run::Column::RetryCount,
            sea_orm::sea_query::Expr::value(new_retry_count).into(),
        )
        .col_expr(
            code_run::Column::JudgeEpoch,
            sea_orm::sea_query::Expr::value(new_epoch).into(),
        )
        .col_expr(
            code_run::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(owner_server_id.clone()).into(),
        )
        .col_expr(
            code_run::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::value(lease_heartbeat_at).into(),
        )
        .clear_judgement_columns()
        .filter(code_run::Column::Id.eq(run.id))
        .filter(code_run::Column::JudgeEpoch.eq(run.judge_epoch))
        .filter(code_run::Column::Status.is_in(STUCK_RECOVERY_STATUSES))
        .exec(txn)
        .await?;

    if affected.rows_affected == 0 {
        return Ok(StuckRecovery::Skip);
    }

    let mut redispatch_model = run.clone();
    redispatch_model.status = SubmissionStatus::Pending;
    redispatch_model.retry_count = new_retry_count;
    redispatch_model.judge_epoch = new_epoch;
    redispatch_model.owner_server_id = owner_server_id;
    redispatch_model.lease_heartbeat_at = lease_heartbeat_at;
    redispatch_model.clear_judgement_fields();

    Ok(StuckRecovery::RedispatchCodeRun {
        model: redispatch_model,
        retry_count: new_retry_count,
    })
}

pub(super) async fn recover_stuck_judgement_without_steal(
    txn: &DatabaseTransaction,
    judgement: &submission_judgement::Model,
    server_id: &str,
) -> anyhow::Result<StuckRecovery> {
    let new_retry_count = judgement.retry_count.saturating_add(1);
    let new_epoch = judgement.judge_epoch.saturating_add(1);
    let lease_heartbeat_at = Utc::now();
    let (owner_server_id, lease_heartbeat_at) = detector_retry_lease(server_id, lease_heartbeat_at);

    let Some(mut sub) = submission::Entity::find_by_id(judgement.submission_id)
        .one(txn)
        .await?
    else {
        return Ok(StuckRecovery::Skip);
    };

    if judgement.is_current {
        let affected = submission::Entity::update_many()
            .col_expr(
                submission::Column::Status,
                sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
            )
            .col_expr(
                submission::Column::RetryCount,
                sea_orm::sea_query::Expr::value(new_retry_count).into(),
            )
            .col_expr(
                submission::Column::JudgeEpoch,
                sea_orm::sea_query::Expr::value(new_epoch).into(),
            )
            .col_expr(
                submission::Column::OwnerServerId,
                sea_orm::sea_query::Expr::value(owner_server_id.clone()).into(),
            )
            .col_expr(
                submission::Column::LeaseHeartbeatAt,
                sea_orm::sea_query::Expr::value(lease_heartbeat_at).into(),
            )
            .clear_judgement_columns()
            .filter(submission::Column::Id.eq(sub.id))
            .filter(submission::Column::JudgeEpoch.eq(judgement.judge_epoch))
            .filter(submission::Column::Status.is_in(STUCK_RECOVERY_STATUSES))
            .exec(txn)
            .await?;
        if affected.rows_affected == 0 {
            return Ok(StuckRecovery::Skip);
        }
    }

    test_case_result::Entity::delete_many()
        .filter(test_case_result::Column::JudgementId.eq(judgement.id))
        .exec(txn)
        .await?;

    let affected = submission_judgement::Entity::update_many()
        .col_expr(
            submission_judgement::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::Pending.to_string()).into(),
        )
        .col_expr(
            submission_judgement::Column::RetryCount,
            sea_orm::sea_query::Expr::value(new_retry_count).into(),
        )
        .col_expr(
            submission_judgement::Column::JudgeEpoch,
            sea_orm::sea_query::Expr::value(new_epoch).into(),
        )
        .col_expr(
            submission_judgement::Column::OwnerServerId,
            sea_orm::sea_query::Expr::value(owner_server_id.clone()).into(),
        )
        .col_expr(
            submission_judgement::Column::LeaseHeartbeatAt,
            sea_orm::sea_query::Expr::value(lease_heartbeat_at).into(),
        )
        .clear_judgement_columns()
        .filter(submission_judgement::Column::Id.eq(judgement.id))
        .filter(submission_judgement::Column::JudgeEpoch.eq(judgement.judge_epoch))
        .filter(submission_judgement::Column::Status.is_in(STUCK_RECOVERY_STATUSES))
        .filter(submission_judgement::Column::IsFinalized.eq(false))
        .exec(txn)
        .await?;

    if affected.rows_affected == 0 {
        anyhow::bail!(
            "stuck judgement recovery parent update succeeded but judgement update matched no rows"
        );
    }
    sub.status = SubmissionStatus::Pending;
    sub.judge_epoch = new_epoch;
    sub.retry_count = new_retry_count;
    sub.owner_server_id = owner_server_id;
    sub.lease_heartbeat_at = lease_heartbeat_at;
    if let Some(target) = judgement.target_worker_id.clone() {
        sub.target_worker_id = Some(target);
    }

    Ok(StuckRecovery::RedispatchJudgement {
        submission: sub,
        judgement_id: judgement.id,
        fire_after_judging: judgement.is_current,
        retry_count: new_retry_count,
    })
}

/// Supersede the abandoned current judgement and insert a fresh current
/// judgement for a detector-owned retry when lease/steal is disabled.
async fn open_retry_submission_judgement(
    txn: &DatabaseTransaction,
    submission: &submission::Model,
    new_epoch: i32,
    owner_server_id: Option<String>,
    lease_heartbeat_at: Option<chrono::DateTime<Utc>>,
) -> anyhow::Result<()> {
    use sea_orm::{ActiveModelTrait, QueryOrder, Set};

    submission_judgement::Entity::update_many()
        .col_expr(
            submission_judgement::Column::IsCurrent,
            sea_orm::sea_query::Expr::value(false).into(),
        )
        .col_expr(
            submission_judgement::Column::IsFinalized,
            sea_orm::sea_query::Expr::value(true).into(),
        )
        .col_expr(
            submission_judgement::Column::Status,
            sea_orm::sea_query::Expr::value(SubmissionStatus::SystemError.to_string()).into(),
        )
        .col_expr(
            submission_judgement::Column::ErrorCode,
            sea_orm::sea_query::Expr::value(Some(SubmissionDlqErrorCode::STUCK_JOB.to_string()))
                .into(),
        )
        .col_expr(
            submission_judgement::Column::ErrorMessage,
            sea_orm::sea_query::Expr::value(Some("Superseded by stuck-job retry".to_string()))
                .into(),
        )
        .col_expr(
            submission_judgement::Column::FinalizedAt,
            sea_orm::sea_query::Expr::cust("NOW()").into(),
        )
        .filter(submission_judgement::Column::SubmissionId.eq(submission.id))
        .filter(submission_judgement::Column::IsCurrent.eq(true))
        .filter(submission_judgement::Column::IsFinalized.eq(false))
        .exec(txn)
        .await?;

    let max_version: Option<i32> = submission_judgement::Entity::find()
        .filter(submission_judgement::Column::SubmissionId.eq(submission.id))
        .order_by_desc(submission_judgement::Column::Version)
        .one(txn)
        .await?
        .map(|j| j.version);

    submission_judgement::ActiveModel {
        submission_id: Set(submission.id),
        version: Set(max_version.unwrap_or(0).saturating_add(1)),
        is_current: Set(true),
        is_finalized: Set(false),
        triggered_by_user_id: Set(None),
        target_worker_id: Set(submission.target_worker_id.clone()),
        note: Set(None),
        status: Set(SubmissionStatus::Pending),
        verdict: Set(None),
        score: Set(None),
        time_used: Set(None),
        memory_used: Set(None),
        compile_output: Set(None),
        error_code: Set(None),
        error_message: Set(None),
        judge_epoch: Set(new_epoch),
        owner_server_id: Set(owner_server_id),
        lease_heartbeat_at: Set(lease_heartbeat_at),
        created_at: Set(chrono::Utc::now()),
        finalized_at: Set(None),
        ..Default::default()
    }
    .insert(txn)
    .await?;

    Ok(())
}
