use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use common::storage::{BlobStore, ContentHash};

#[cfg(unix)]
fn ensure_readable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mode = meta.permissions().mode();
        if mode & 0o044 != 0o044
            && let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))
        {
            tracing::warn!(path = %path.display(), error = %e, "Failed to set readable permissions on cached file");
        }
    }
}

#[cfg(not(unix))]
fn ensure_readable(_path: &Path) {}

#[async_trait]
pub trait FileCacher: Send + Sync {
    async fn fetch_to_path(&self, content_hash: &str, dest: &Path) -> Result<(), String>;

    async fn upload_from_path(&self, src: &Path) -> Result<String, String>;
}

pub struct NoopFileCacher;

#[async_trait]
impl FileCacher for NoopFileCacher {
    async fn fetch_to_path(&self, _content_hash: &str, _dest: &Path) -> Result<(), String> {
        Ok(())
    }
    async fn upload_from_path(&self, _src: &Path) -> Result<String, String> {
        Ok("0".repeat(64))
    }
}

#[allow(dead_code)]
pub struct UnavailableFileCacher {
    reason: String,
}

#[allow(dead_code)]
impl UnavailableFileCacher {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[async_trait]
impl FileCacher for UnavailableFileCacher {
    async fn fetch_to_path(&self, content_hash: &str, _dest: &Path) -> Result<(), String> {
        Err(format!(
            "blob storage is unavailable; cannot fetch blob {content_hash}: {}",
            self.reason
        ))
    }

    async fn upload_from_path(&self, src: &Path) -> Result<String, String> {
        Err(format!(
            "blob storage is unavailable; cannot upload {}: {}",
            src.display(),
            self.reason
        ))
    }
}

struct FetchLockCleanup<'a>(&'a BlobStoreFileCacher, String);

impl Drop for FetchLockCleanup<'_> {
    fn drop(&mut self) {
        self.0.remove_fetch_lock(&self.1);
    }
}

pub struct BlobStoreFileCacher {
    store: Arc<dyn BlobStore>,
    cache_dir: PathBuf,
    max_cache_size: u64,

    state: tokio::sync::Mutex<CacheState>,
    fetch_locks: std::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

struct CacheState {
    entries: lru::LruCache<String, u64>,
    total_size: u64,
}

impl BlobStoreFileCacher {
    pub async fn new(
        store: Arc<dyn BlobStore>,
        cache_dir: PathBuf,
        max_cache_size: u64,
    ) -> Result<Self, String> {
        tokio::fs::create_dir_all(&cache_dir)
            .await
            .map_err(|e| format!("Failed to create cache dir: {e}"))?;

        let mut entries_vec = Vec::new();
        let mut total_size: u64 = 0;
        let mut rd = tokio::fs::read_dir(&cache_dir)
            .await
            .map_err(|e| format!("Failed to read cache dir: {e}"))?;

        while let Ok(Some(entry)) = rd.next_entry().await {
            if let Ok(meta) = entry.metadata().await
                && meta.is_file()
            {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".tmp") {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                    continue;
                }
                let size = meta.len();
                let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                entries_vec.push((name, size, mtime));
                total_size += size;
            }
        }

        entries_vec.sort_by_key(|(_, _, mtime)| *mtime);

        let mut entries = lru::LruCache::unbounded();
        for (name, size, _) in entries_vec {
            entries.put(name, size);
        }

        let cacher = Self {
            store,
            cache_dir,
            max_cache_size,
            state: tokio::sync::Mutex::new(CacheState {
                entries,
                total_size,
            }),
            fetch_locks: std::sync::Mutex::new(std::collections::HashMap::new()),
        };

        cacher.evict_if_needed().await;

        Ok(cacher)
    }

    fn cache_path(&self, hash_hex: &str) -> PathBuf {
        self.cache_dir.join(hash_hex)
    }

    fn get_fetch_lock(&self, hash_hex: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.fetch_locks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(lock) = locks.get(hash_hex) {
            lock.clone()
        } else {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            locks.insert(hash_hex.to_string(), lock.clone());
            lock
        }
    }

    fn remove_fetch_lock(&self, hash_hex: &str) {
        if let Ok(mut locks) = self.fetch_locks.lock() {
            locks.remove(hash_hex);
        }
    }

    #[cfg(test)]
    pub async fn current_size(&self) -> u64 {
        self.state.lock().await.total_size
    }

    async fn evict_if_needed(&self) {
        let mut state = self.state.lock().await;
        while state.total_size > self.max_cache_size && !state.entries.is_empty() {
            if let Some((hash, size)) = state.entries.pop_lru() {
                let path = self.cache_dir.join(&hash);
                if let Err(e) = tokio::fs::remove_file(&path).await {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to evict cached file");
                } else {
                    state.total_size = state.total_size.saturating_sub(size);
                }
            }
        }
    }

    async fn record_cache_entry(&self, hash_hex: String, size: u64) {
        {
            let mut state = self.state.lock().await;
            let old_size = state.entries.put(hash_hex, size).unwrap_or(0);
            state.total_size = state.total_size + size - old_size;
        }
        self.evict_if_needed().await;
    }

    async fn touch(&self, hash_hex: &str) {
        let mut state = self.state.lock().await;
        state.entries.get(hash_hex);
    }
}

#[async_trait]
impl FileCacher for BlobStoreFileCacher {
    async fn fetch_to_path(&self, content_hash: &str, dest: &Path) -> Result<(), String> {
        let hash = ContentHash::from_hex(content_hash).map_err(|e| e.to_string())?;
        let hash_hex = hash.to_hex();
        let cached = self.cache_path(&hash_hex);

        if cached.exists() {
            self.touch(&hash_hex).await;
            ensure_readable(&cached);
            tokio::fs::copy(&cached, dest)
                .await
                .map_err(|e| format!("Failed to copy cached file: {e}"))?;
            return Ok(());
        }

        let lock = self.get_fetch_lock(&hash_hex);
        let _cleanup = FetchLockCleanup(self, hash_hex.clone());
        let _guard = lock.lock().await;

        if cached.exists() {
            self.touch(&hash_hex).await;
            ensure_readable(&cached);
            tokio::fs::copy(&cached, dest)
                .await
                .map_err(|e| format!("Failed to copy cached file: {e}"))?;
            return Ok(());
        }

        let temp_path = self.cache_dir.join(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut temp_file = tokio::fs::File::create(&temp_path).await.map_err(|e| {
            format!(
                "Failed to create temp cache file {}: {}",
                temp_path.display(),
                e
            )
        })?;

        let mut reader = self
            .store
            .get_stream(&hash)
            .await
            .map_err(|e| e.to_string())?;

        let file_size = tokio::io::copy(&mut reader, &mut temp_file)
            .await
            .map_err(|e| {
                let temp_path_clone = temp_path.clone();
                tokio::spawn(async move {
                    let _ = tokio::fs::remove_file(temp_path_clone).await;
                });
                format!("Failed to stream blob to cache: {e}")
            })?;

        tokio::fs::rename(&temp_path, &cached).await.map_err(|e| {
            let temp_path_clone = temp_path.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_file(temp_path_clone).await;
            });
            format!("Failed to finalize cache file: {e}")
        })?;

        ensure_readable(&cached);
        self.record_cache_entry(hash_hex.clone(), file_size).await;

        tokio::fs::copy(&cached, dest)
            .await
            .map_err(|e| format!("Failed to copy to dest: {e}"))?;

        Ok(())
    }

    async fn upload_from_path(&self, src: &Path) -> Result<String, String> {
        let file = tokio::fs::File::open(src)
            .await
            .map_err(|e| format!("Failed to open file: {e}"))?;

        let meta = tokio::fs::metadata(src)
            .await
            .map_err(|e| format!("Failed to read metadata: {e}"))?;
        let file_size = meta.len();

        let reader: common::storage::BoxReader = Box::new(file);
        let hash = self
            .store
            .put_stream(reader)
            .await
            .map_err(|e| e.to_string())?;

        let hash_hex = hash.to_hex();
        let cached = self.cache_path(&hash_hex);

        if !cached.exists() {
            let cached_ok = tokio::fs::copy(src, &cached).await.is_ok();
            if cached_ok {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(&cached, std::fs::Permissions::from_mode(0o644));
                }
                self.record_cache_entry(hash_hex.clone(), file_size).await;
            }
        } else {
            self.touch(&hash_hex).await;
        }

        Ok(hash_hex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::storage::filesystem::FilesystemBlobStore;

    async fn temp_cacher(
        max_cache: u64,
    ) -> (
        BlobStoreFileCacher,
        tempfile::TempDir,
        Arc<FilesystemBlobStore>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let blob_dir = dir.path().join("blobs");
        let cache_dir = dir.path().join("cache");
        let store = Arc::new(
            FilesystemBlobStore::new(blob_dir, 10 * 1024 * 1024)
                .await
                .unwrap(),
        );
        let cacher = BlobStoreFileCacher::new(store.clone(), cache_dir, max_cache)
            .await
            .unwrap();
        (cacher, dir, store)
    }

    #[tokio::test]
    async fn noop_cacher_works() {
        let c = NoopFileCacher;
        let dest = std::env::temp_dir().join("noop_test");
        assert!(
            c.fetch_to_path("a".repeat(64).as_str(), &dest)
                .await
                .is_ok()
        );
        let h = c.upload_from_path(Path::new("/dev/null")).await.unwrap();
        assert_eq!(h.len(), 64);
    }

    #[tokio::test]
    async fn unavailable_cacher_fails_blob_materialization() {
        let c = UnavailableFileCacher::new("storage init failed");
        let dest = std::env::temp_dir().join("unavailable_test");
        let err = c
            .fetch_to_path("a".repeat(64).as_str(), &dest)
            .await
            .expect_err("fetching a blob with unavailable storage must fail");

        assert!(err.contains("blob storage is unavailable"));
        assert!(err.contains("storage init failed"));
    }

    #[tokio::test]
    async fn upload_and_fetch_round_trip() {
        let (cacher, dir, _store) = temp_cacher(10 * 1024 * 1024).await;

        let src = dir.path().join("source.txt");
        tokio::fs::write(&src, b"hello blob").await.unwrap();
        let hash = cacher.upload_from_path(&src).await.unwrap();

        let dest = dir.path().join("fetched.txt");
        cacher.fetch_to_path(&hash, &dest).await.unwrap();

        let content = tokio::fs::read_to_string(&dest).await.unwrap();
        assert_eq!(content, "hello blob");
    }

    #[tokio::test]
    async fn eviction_keeps_cache_under_limit() {
        let (cacher, dir, _store) = temp_cacher(20).await;

        for i in 0..3u8 {
            let src = dir.path().join(format!("f{i}"));
            tokio::fs::write(&src, vec![i; 10]).await.unwrap();
            cacher.upload_from_path(&src).await.unwrap();
        }

        let total = cacher.current_size().await;
        assert!(total <= 20, "cache size {total} exceeds limit 20");
    }
}
