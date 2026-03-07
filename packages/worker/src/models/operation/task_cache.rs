use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{EntityTrait, Set};

/// Caches entire step outputs keyed by
/// (input files hash + architecture + argv).
#[async_trait]
pub trait TaskCacheStore: Send + Sync {
    /// Look up cached step outputs. Returns map of filename -> content_hash hex.
    async fn get(&self, cache_key: &str) -> Result<Option<HashMap<String, String>>, String>;

    /// Store step outputs after successful execution.
    async fn put(&self, cache_key: &str, outputs: HashMap<String, String>) -> Result<(), String>;
}

/// Database-backed task cache store using an inline SeaORM entity.
pub struct DatabaseTaskCacheStore {
    db: sea_orm::DatabaseConnection,
}

impl DatabaseTaskCacheStore {
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn ensure_table(db: &sea_orm::DatabaseConnection) -> Result<(), String> {
        use sea_orm::{ConnectionTrait, Schema};
        let backend = db.get_database_backend();
        let schema = Schema::new(backend);
        let mut create_stmt = schema.create_table_from_entity(task_cache::Entity);
        create_stmt.if_not_exists();

        db.execute(&create_stmt)
            .await
            .map_err(|e| format!("Failed to ensure task_cache table: {e}"))?;
        Ok(())
    }
}

#[async_trait]
impl TaskCacheStore for DatabaseTaskCacheStore {
    async fn get(&self, cache_key: &str) -> Result<Option<HashMap<String, String>>, String> {
        let result = task_cache::Entity::find_by_id(cache_key.to_string())
            .one(&self.db)
            .await
            .map_err(|e| format!("task_cache lookup failed: {e}"))?;

        match result {
            Some(model) => {
                let outputs: HashMap<String, String> = serde_json::from_value(model.outputs)
                    .map_err(|e| format!("Failed to deserialize cached outputs: {e}"))?;
                Ok(Some(outputs))
            }
            None => Ok(None),
        }
    }

    async fn put(&self, cache_key: &str, outputs: HashMap<String, String>) -> Result<(), String> {
        let outputs_json = serde_json::to_value(&outputs)
            .map_err(|e| format!("Failed to serialize outputs: {e}"))?;

        let model = task_cache::ActiveModel {
            cache_key: Set(cache_key.to_string()),
            outputs: Set(outputs_json),
            created_at: Set(Utc::now()),
        };

        let result = task_cache::Entity::insert(model)
            .on_conflict(
                OnConflict::column(task_cache::Column::CacheKey)
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await;

        match result {
            Ok(_) => {}
            Err(sea_orm::DbErr::RecordNotInserted) => {}
            Err(e) => return Err(format!("task_cache insert failed: {e}")),
        }

        Ok(())
    }
}

/// No-op task cache for tests. Will always return cache miss.
pub struct NoopTaskCacheStore;

#[async_trait]
impl TaskCacheStore for NoopTaskCacheStore {
    async fn get(&self, _cache_key: &str) -> Result<Option<HashMap<String, String>>, String> {
        Ok(None)
    }

    async fn put(&self, _cache_key: &str, _outputs: HashMap<String, String>) -> Result<(), String> {
        Ok(())
    }
}

pub mod task_cache {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[sea_orm::model]
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "task_cache")]
    pub struct Model {
        /// SHA-256 cache key encoding (target triple + argv + input file contents).
        #[sea_orm(primary_key, auto_increment = false)]
        pub cache_key: String,

        /// JSON map of output filename -> content_hash hex.
        #[sea_orm(column_type = "JsonBinary")]
        pub outputs: serde_json::Value,

        pub created_at: DateTimeUtc,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// The full target triple (e.g. `x86_64-unknown-linux-gnu`), injected at compile
/// time via `build.rs`.
const TARGET_TRIPLE: &str = env!("TARGET_TRIPLE");

/// Compute a deterministic cache key for a step.
///
/// Key = SHA256(target_triple + "\0" + toolchain_fingerprint + "\0" + argv_joined + "\0" + sorted(filename + "\0" + content_len + "\0" + content))
///
/// Pass `""` as `toolchain_fingerprint` when version probing is disabled or in tests.
pub fn compute_cache_key(
    toolchain_fingerprint: &str,
    argv: &[String],
    input_files: &[(String, Vec<u8>)],
) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    hasher.update(TARGET_TRIPLE.as_bytes());
    hasher.update(b"\0");

    hasher.update(toolchain_fingerprint.as_bytes());
    hasher.update(b"\0");

    let argv_joined = argv.join("\0");
    hasher.update(argv_joined.as_bytes());
    hasher.update(b"\0");

    let mut sorted_inputs: Vec<_> = input_files.iter().collect();
    sorted_inputs.sort_by_key(|(name, _)| name.as_str());

    for (name, content) in sorted_inputs {
        hasher.update(name.as_bytes());
        hasher.update(b"\0");
        hasher.update(content.len().to_string().as_bytes());
        hasher.update(b"\0");
        hasher.update(content);
        hasher.update(b"\0");
    }

    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_deterministic() {
        let argv = vec!["gcc".into(), "-o".into(), "out".into()];
        let files = vec![
            ("main.c".into(), b"int main() {}".to_vec()),
            ("helper.h".into(), b"void help();".to_vec()),
        ];
        let key1 = compute_cache_key("", &argv, &files);
        let key2 = compute_cache_key("", &argv, &files);
        assert_eq!(key1, key2);
    }

    #[test]
    fn cache_key_order_independent() {
        let argv = vec!["gcc".into()];
        let files_a = vec![
            ("a.c".into(), b"aaa".to_vec()),
            ("b.c".into(), b"bbb".to_vec()),
        ];
        let files_b = vec![
            ("b.c".into(), b"bbb".to_vec()),
            ("a.c".into(), b"aaa".to_vec()),
        ];
        assert_eq!(
            compute_cache_key("", &argv, &files_a),
            compute_cache_key("", &argv, &files_b)
        );
    }

    #[test]
    fn cache_key_differs_for_different_content() {
        let argv = vec!["gcc".into()];
        let files1 = vec![("main.c".into(), b"version 1".to_vec())];
        let files2 = vec![("main.c".into(), b"version 2".to_vec())];
        assert_ne!(
            compute_cache_key("", &argv, &files1),
            compute_cache_key("", &argv, &files2)
        );
    }

    #[test]
    fn cache_key_differs_for_different_fingerprints() {
        let argv = vec!["gcc".into()];
        let files = vec![("main.c".into(), b"int main() {}".to_vec())];
        let key_a = compute_cache_key("fingerprint-a", &argv, &files);
        let key_b = compute_cache_key("fingerprint-b", &argv, &files);
        assert_ne!(key_a, key_b);
    }

    #[tokio::test]
    async fn noop_cache_always_misses() {
        let cache = NoopTaskCacheStore;
        assert!(cache.get("any_key").await.unwrap().is_none());
        assert!(cache.put("any_key", HashMap::new()).await.is_ok());
    }
}
