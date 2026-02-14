use std::collections::HashMap;

use chrono::Utc;
use common::{DlqEnvelope, DlqErrorCode, DlqMessageType};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr, sea_query::LockType,
};

use crate::entity::dead_letter_message;

/// Result of attempting to resolve a DLQ message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveResult {
    /// Message was successfully resolved.
    Resolved,
    /// Message was not found.
    NotFound,
    /// Message was already resolved.
    AlreadyResolved,
}

/// Statistics about the dead letter queue.
#[derive(Debug, Clone)]
pub struct DlqStats {
    pub total_unresolved: u64,
    pub total_resolved: u64,
    pub judge_job_count: u64,
    pub judge_result_count: u64,
    /// Unresolved message count grouped by error code.
    pub unresolved_by_error_code: HashMap<String, u64>,
}

pub struct DlqService<'a, C: ConnectionTrait> {
    conn: &'a C,
}

impl<'a, C: ConnectionTrait> DlqService<'a, C> {
    pub fn new(conn: &'a C) -> Self {
        Self { conn }
    }

    /// Persist a failed message to the DLQ.
    pub async fn send_to_dlq(
        &self,
        envelope: &DlqEnvelope,
    ) -> Result<dead_letter_message::Model, DbErr> {
        let first_failed_at = envelope
            .retry_history
            .first()
            .map(|r| r.timestamp)
            .unwrap_or_else(Utc::now);

        let model = dead_letter_message::ActiveModel {
            message_id: Set(envelope.message_id.clone()),
            message_type: Set(envelope.message_type.to_string()),
            submission_id: Set(envelope.submission_id),
            payload: Set(envelope.payload.clone()),
            error_message: Set(envelope.error_message.clone()),
            error_code: Set(envelope.error_code.to_string()),
            retry_count: Set(envelope.retry_history.len() as i32),
            retry_history: Set(serde_json::to_value(&envelope.retry_history).unwrap_or_default()),
            first_failed_at: Set(first_failed_at),
            created_at: Set(Utc::now()),
            resolved: Set(false),
            resolved_at: Set(None),
            resolved_by: Set(None),
            ..Default::default()
        };

        self.insert_entry(&envelope.message_id, model).await
    }

    /// Create a DLQ entry directly from components.
    pub async fn create_entry(
        &self,
        message_id: String,
        message_type: DlqMessageType,
        submission_id: Option<i32>,
        payload: serde_json::Value,
        error_code: DlqErrorCode,
        error_message: String,
    ) -> Result<dead_letter_message::Model, DbErr> {
        let now = Utc::now();
        let model = dead_letter_message::ActiveModel {
            message_id: Set(message_id.clone()),
            message_type: Set(message_type.to_string()),
            submission_id: Set(submission_id),
            payload: Set(payload),
            error_message: Set(error_message),
            error_code: Set(error_code.to_string()),
            retry_count: Set(0),
            retry_history: Set(serde_json::json!([])),
            first_failed_at: Set(now),
            created_at: Set(now),
            resolved: Set(false),
            resolved_at: Set(None),
            resolved_by: Set(None),
            ..Default::default()
        };

        self.insert_entry(&message_id, model).await
    }

    /// Insert a DLQ entry.
    async fn insert_entry(
        &self,
        message_id: &str,
        model: dead_letter_message::ActiveModel,
    ) -> Result<dead_letter_message::Model, DbErr> {
        match model.insert(self.conn).await {
            Ok(inserted) => Ok(inserted),
            Err(e) if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
                dead_letter_message::Entity::find()
                    .filter(dead_letter_message::Column::MessageId.eq(message_id))
                    .one(self.conn)
                    .await?
                    .ok_or_else(|| {
                        DbErr::Custom(
                            "UniqueConstraintViolation but existing row not found".to_string(),
                        )
                    })
            }
            Err(e) => Err(e),
        }
    }

    /// List DLQ messages.
    pub async fn list(
        &self,
        message_type: Option<DlqMessageType>,
        resolved: Option<bool>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<dead_letter_message::Model>, u64), DbErr> {
        let mut query = dead_letter_message::Entity::find();

        if let Some(mt) = message_type {
            query = query.filter(dead_letter_message::Column::MessageType.eq(mt.to_string()));
        }

        if let Some(res) = resolved {
            query = query.filter(dead_letter_message::Column::Resolved.eq(res));
        }

        let total = query.clone().count(self.conn).await?;

        let messages = query
            .order_by_desc(dead_letter_message::Column::CreatedAt)
            .offset((page.saturating_sub(1)) * per_page)
            .limit(per_page)
            .all(self.conn)
            .await?;

        Ok((messages, total))
    }

    /// Get a single DLQ message by ID.
    pub async fn get_by_id(&self, id: i32) -> Result<Option<dead_letter_message::Model>, DbErr> {
        dead_letter_message::Entity::find_by_id(id)
            .one(self.conn)
            .await
    }

    /// Get a single DLQ message by ID with FOR UPDATE lock.
    pub async fn get_by_id_for_update(
        &self,
        id: i32,
    ) -> Result<Option<dead_letter_message::Model>, DbErr> {
        dead_letter_message::Entity::find_by_id(id)
            .lock(LockType::Update)
            .one(self.conn)
            .await
    }

    /// Mark a message as resolved.
    pub async fn resolve(&self, id: i32, resolved_by: Option<i32>) -> Result<ResolveResult, DbErr> {
        let update = dead_letter_message::Entity::update_many()
            .col_expr(
                dead_letter_message::Column::Resolved,
                sea_orm::sea_query::Expr::value(true),
            )
            .col_expr(
                dead_letter_message::Column::ResolvedAt,
                sea_orm::sea_query::Expr::value(Utc::now()),
            )
            .col_expr(
                dead_letter_message::Column::ResolvedBy,
                sea_orm::sea_query::Expr::value(resolved_by),
            )
            .filter(dead_letter_message::Column::Id.eq(id))
            .filter(dead_letter_message::Column::Resolved.eq(false));

        let update_result = update.exec(self.conn).await?;

        if update_result.rows_affected > 0 {
            return Ok(ResolveResult::Resolved);
        }

        let exists = dead_letter_message::Entity::find_by_id(id)
            .one(self.conn)
            .await?
            .is_some();

        if exists {
            Ok(ResolveResult::AlreadyResolved)
        } else {
            Ok(ResolveResult::NotFound)
        }
    }

    /// Get DLQ statistics.
    pub async fn stats(&self) -> Result<DlqStats, DbErr> {
        let total_resolved = dead_letter_message::Entity::find()
            .filter(dead_letter_message::Column::Resolved.eq(true))
            .count(self.conn)
            .await?;

        let unresolved_data: Vec<(String, String)> = dead_letter_message::Entity::find()
            .select_only()
            .column(dead_letter_message::Column::MessageType)
            .column(dead_letter_message::Column::ErrorCode)
            .filter(dead_letter_message::Column::Resolved.eq(false))
            .into_tuple()
            .all(self.conn)
            .await?;

        let total_unresolved = unresolved_data.len() as u64;
        let mut judge_job_count = 0u64;
        let mut judge_result_count = 0u64;
        let mut unresolved_by_error_code: HashMap<String, u64> = HashMap::new();

        for (message_type, error_code) in unresolved_data {
            match message_type.as_str() {
                "judge_job" => judge_job_count += 1,
                "judge_result" => judge_result_count += 1,
                _ => {}
            }
            *unresolved_by_error_code.entry(error_code).or_insert(0) += 1;
        }

        Ok(DlqStats {
            total_unresolved,
            total_resolved,
            judge_job_count,
            judge_result_count,
            unresolved_by_error_code,
        })
    }

    /// Resolve multiple DLQ messages at once. Returns the number of rows affected.
    pub async fn resolve_many(&self, ids: &[i32], resolved_by: Option<i32>) -> Result<u64, DbErr> {
        let result = dead_letter_message::Entity::update_many()
            .col_expr(
                dead_letter_message::Column::Resolved,
                sea_orm::sea_query::Expr::value(true),
            )
            .col_expr(
                dead_letter_message::Column::ResolvedAt,
                sea_orm::sea_query::Expr::value(Utc::now()),
            )
            .col_expr(
                dead_letter_message::Column::ResolvedBy,
                sea_orm::sea_query::Expr::value(resolved_by),
            )
            .filter(dead_letter_message::Column::Id.is_in(ids.to_vec()))
            .filter(dead_letter_message::Column::Resolved.eq(false))
            .exec(self.conn)
            .await?;

        Ok(result.rows_affected)
    }

    /// Check if a submission already has an unresolved DLQ entry.
    pub async fn has_unresolved_entry(&self, submission_id: i32) -> Result<bool, DbErr> {
        let count = dead_letter_message::Entity::find()
            .filter(dead_letter_message::Column::SubmissionId.eq(submission_id))
            .filter(dead_letter_message::Column::Resolved.eq(false))
            .count(self.conn)
            .await?;

        Ok(count > 0)
    }
}

/// Create a DlqService with a DatabaseConnection.
pub fn dlq_service(db: &DatabaseConnection) -> DlqService<'_, DatabaseConnection> {
    DlqService::new(db)
}
