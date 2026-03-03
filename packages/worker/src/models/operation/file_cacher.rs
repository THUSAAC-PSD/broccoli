use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use common::storage::{BlobStore, ContentHash};

/// Abstracts file fetch/upload for the worker.
#[async_trait]
pub trait FileCacher: Send + Sync {
    /// Fetch a blob by content hash and write it to `dest`.
    async fn fetch_to_path(&self, content_hash: &str, dest: &Path) -> Result<(), String>;

    /// Upload a file and return its content hash hex string.
    async fn upload_from_path(&self, src: &Path) -> Result<String, String>;
}

/// No-op implementation for tests.
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

/// File cacher backed by a [`BlobStore`] with LRU disk cache.
pub struct BlobStoreFileCacher {
    store: Arc<dyn BlobStore>,
    cache_dir: PathBuf,
    max_cache_size: u64,

    /// LRU cache state and total size tracking.
    state: tokio::sync::Mutex<CacheState>,
    /// Per-file lock to prevent concurrent downloads of the same file.
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

        // Scan existing cache entries and sort by modification time to approximate LRU on restart.
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
                // Skip temporary files from partial downloads.
                if name.ends_with(".tmp") {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                    continue;
                }
                let size = meta.len();
                // Use modified time for sorting, fallback to 0 if unavailable
                let mtime = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                entries_vec.push((name, size, mtime));
                total_size += size;
            }
        }

        // Sort oldest first
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

        // Evict if over limit on startup.
        cacher.evict_if_needed().await;

        Ok(cacher)
    }

    fn cache_path(&self, hash_hex: &str) -> PathBuf {
        self.cache_dir.join(hash_hex)
    }

    fn get_fetch_lock(&self, hash_hex: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.fetch_locks.lock().unwrap();
        if let Some(lock) = locks.get(hash_hex) {
            lock.clone()
        } else {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            locks.insert(hash_hex.to_string(), lock.clone());
            lock
        }
    }

    fn remove_fetch_lock(&self, hash_hex: &str) {
        let mut locks = self.fetch_locks.lock().unwrap();
        locks.remove(hash_hex);
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
                let _ = tokio::fs::remove_file(&path).await;
                state.total_size = state.total_size.saturating_sub(size);
            }
        }
    }

    /// Add an entry to the cache and perform eviction if needed.
    async fn record_cache_entry(&self, hash_hex: String, size: u64) {
        {
            let mut state = self.state.lock().await;
            // It might already exist, get its old size
            let old_size = state.entries.put(hash_hex, size).unwrap_or(0);
            state.total_size = state.total_size + size - old_size;
        }
        self.evict_if_needed().await;
    }

    /// Mark an entry as most recently used.
    async fn touch(&self, hash_hex: &str) {
        let mut state = self.state.lock().await;
        state.entries.get(hash_hex);
    }
}

#[async_trait]
impl FileCacher for BlobStoreFileCacher {
    async fn fetch_to_path(&self, content_hash: &str, dest: &Path) -> Result<(), String> {
        // Validate hash format.
        let hash = ContentHash::from_hex(content_hash).map_err(|e| e.to_string())?;
        let hash_hex = hash.to_hex();
        let cached = self.cache_path(&hash_hex);

        // First non-blocking check for cache hit
        if cached.exists() {
            self.touch(&hash_hex).await;
            tokio::fs::copy(&cached, dest)
                .await
                .map_err(|e| format!("Failed to copy cached file: {e}"))?;
            return Ok(());
        }

        // Acquire lock for this specific hash to prevent concurrent downloads
        let lock = self.get_fetch_lock(&hash_hex);
        let _guard = lock.lock().await;

        // Check again in case another task downloaded it while we were waiting
        if cached.exists() {
            self.remove_fetch_lock(&hash_hex);
            self.touch(&hash_hex).await;
            tokio::fs::copy(&cached, dest)
                .await
                .map_err(|e| format!("Failed to copy cached file: {e}"))?;
            return Ok(());
        }

        // Cache miss — fetch from store directly to a temporary file in cache dir
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
                // Try to clean up temp file on failure
                let temp_path_clone = temp_path.clone();
                tokio::spawn(async move {
                    let _ = tokio::fs::remove_file(temp_path_clone).await;
                });
                format!("Failed to stream blob to cache: {e}")
            })?;

        // Rename temp file to final cache file atomically
        tokio::fs::rename(&temp_path, &cached).await.map_err(|e| {
            let temp_path_clone = temp_path.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_file(temp_path_clone).await;
            });
            format!("Failed to finalize cache file: {e}")
        })?;

        // Successfully downloaded, update cache state
        self.record_cache_entry(hash_hex.clone(), file_size).await;

        // Remove the lock since we're done
        drop(_guard);
        self.remove_fetch_lock(&hash_hex);

        // Copy to destination (or attempt a hard link if possible, falling back to copy)
        if std::fs::hard_link(&cached, dest).is_err() {
            tokio::fs::copy(&cached, dest)
                .await
                .map_err(|e| format!("Failed to copy to dest: {e}"))?;
        }

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

        // Also cache it locally if not already present.
        let hash_hex = hash.to_hex();
        let cached = self.cache_path(&hash_hex);

        if !cached.exists() {
            // Attempt to hardlink from source to cache to avoid data copy
            if std::fs::hard_link(src, &cached).is_err() {
                // Fall back to copying if hard link fails (e.g. across mount points)
                let _ = tokio::fs::copy(src, &cached).await;
            }
            self.record_cache_entry(hash_hex.clone(), file_size).await;
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
    async fn upload_and_fetch_round_trip() {
        let (cacher, dir, _store) = temp_cacher(10 * 1024 * 1024).await;

        // Write a file and upload it.
        let src = dir.path().join("source.txt");
        tokio::fs::write(&src, b"hello blob").await.unwrap();
        let hash = cacher.upload_from_path(&src).await.unwrap();

        // Fetch it back.
        let dest = dir.path().join("fetched.txt");
        cacher.fetch_to_path(&hash, &dest).await.unwrap();

        let content = tokio::fs::read_to_string(&dest).await.unwrap();
        assert_eq!(content, "hello blob");
    }

    #[tokio::test]
    async fn cache_hit_avoids_second_blob_read() {
        let (cacher, dir, _store) = temp_cacher(10 * 1024 * 1024).await;

        let src = dir.path().join("data.bin");
        tokio::fs::write(&src, b"cached data").await.unwrap();
        let hash = cacher.upload_from_path(&src).await.unwrap();

        // Fetch twice — second should use cache.
        let d1 = dir.path().join("d1");
        let d2 = dir.path().join("d2");
        cacher.fetch_to_path(&hash, &d1).await.unwrap();
        cacher.fetch_to_path(&hash, &d2).await.unwrap();

        assert_eq!(tokio::fs::read_to_string(&d1).await.unwrap(), "cached data");
        assert_eq!(tokio::fs::read_to_string(&d2).await.unwrap(), "cached data");
    }

    #[tokio::test]
    async fn eviction_keeps_cache_under_limit() {
        // Cache limit = 20 bytes.
        let (cacher, dir, _store) = temp_cacher(20).await;

        // Upload three 10-byte files — total would be 30, limit is 20.
        for i in 0..3u8 {
            let src = dir.path().join(format!("f{i}"));
            tokio::fs::write(&src, vec![i; 10]).await.unwrap();
            cacher.upload_from_path(&src).await.unwrap();
        }

        let total = cacher.current_size().await;
        assert!(total <= 20, "cache size {total} exceeds limit 20");
    }
}
