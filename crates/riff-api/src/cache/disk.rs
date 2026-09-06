//! File-based TTL cache with ETag revalidation support.
//!
//! Keys are hashed into a UUID rather than used as filenames directly, so no key can
//! escape the cache directory. Each cached resource is stored as two files:
//! - `{uuid}` - the raw bytes
//! - `{uuid}.expiry` - 8-byte unix timestamp (big-endian u64) + optional ETag string

use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use riff_config::EXPIRY_EXT;
use tokio::fs;
use uuid::Uuid;

/// Namespace for cache entry UUIDs. Update this value if making a non-backwards compatible
/// change to the cache.
const NAMESPACE_URL: Uuid = Uuid::NAMESPACE_URL;

/// Derive a deterministic UUIDv5 from a cache key.
#[allow(dead_code)] // Not wired up yet.
fn key_to_uuid(key: &str) -> Uuid {
    Uuid::new_v5(&NAMESPACE_URL, key.as_bytes())
}

pub struct DiskEntry {
    pub data: Box<[u8]>,
    pub state: EntryState,
}

pub enum EntryState {
    Fresh,
    Stale { etag: Option<String> },
}

#[derive(Clone)]
pub struct DiskCache {
    root: PathBuf,
    max_bytes: usize,
    default_ttl: Duration,
}

impl DiskCache {
    pub fn new(subdir: &str, max_bytes: usize, default_ttl: Duration) -> Self {
        let root: PathBuf = glib::user_cache_dir();
        let root = root.join(subdir);
        // Owner-only: cache may hold ETags and API payloads tied to the user.
        if let Err(e) = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&root)
        {
            warn!("disk cache: failed to create {}: {e}", root.display());
        }
        Self {
            root,
            max_bytes,
            default_ttl,
        }
    }

    pub async fn read(&self, key: &str) -> Option<DiskEntry> {
        let file_uuid = key_to_uuid(key);
        let path = self.root.join(file_uuid.to_string());
        let (data_result, expiry) = tokio::join!(fs::read(&path), self.read_expiry(&file_uuid));
        let data = data_result.ok()?;
        let data = data.into_boxed_slice();
        let state = match expiry {
            Some((ts, _etag)) if !is_expired(ts) => EntryState::Fresh,
            Some((_, etag)) => EntryState::Stale { etag },
            None => EntryState::Stale { etag: None },
        };
        Some(DiskEntry { data, state })
    }

    pub async fn write(&self, key: &str, data: &[u8], ttl: Duration, etag: Option<&str>) {
        let file_uuid = key_to_uuid(key);
        let path = self.root.join(file_uuid.to_string());
        let expiry_content = Self::build_expiry_content(ttl, etag);
        let expiry_path = self.root.join(format!("{file_uuid}{EXPIRY_EXT}"));

        let (data_res, expiry_res) = tokio::join!(
            fs::write(&path, data),
            fs::write(&expiry_path, &expiry_content)
        );
        if let Err(e) = data_res {
            warn!("disk cache: failed to write {key} ({file_uuid}): {e}");
            let _ = fs::remove_file(&expiry_path).await;
            return;
        }
        if let Err(e) = expiry_res {
            warn!("disk cache: failed to write expiry for {key} ({file_uuid}): {e}");
        }
    }

    pub async fn write_default(&self, key: &str, data: &[u8], etag: Option<&str>) {
        self.write(key, data, self.default_ttl, etag).await;
    }

    /// Refresh the TTL without rewriting data. Preserves existing ETag if `etag` is None.
    pub async fn refresh_ttl(&self, key: &str, ttl: Duration, etag: Option<&str>) {
        let file_uuid = key_to_uuid(key);
        let existing_etag = if etag.is_none() {
            self.read_expiry(&file_uuid).await.and_then(|(_, e)| e)
        } else {
            None
        };
        let effective_etag = etag.or(existing_etag.as_deref());
        self.write_expiry(&file_uuid, ttl, effective_etag).await;
    }

    pub async fn invalidate(&self, key: &str) {
        let file_uuid = key_to_uuid(key);
        let _ = fs::remove_file(self.root.join(file_uuid.to_string())).await;
        let _ = fs::remove_file(self.root.join(format!("{file_uuid}{EXPIRY_EXT}"))).await;
    }

    pub async fn clear(&self) {
        let Ok(mut entries) = fs::read_dir(&self.root)
            .await
            .inspect_err(|e| warn!("disk cache: cannot read {}: {e}", self.root.display()))
        else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let _ = fs::remove_file(entry.path()).await;
        }
    }

    /// Evict oldest entries until total size is within budget.
    pub async fn evict_to_budget(&self) {
        let mut entries: Vec<(String, usize, SystemTime)> = Vec::new();
        let mut total_bytes = 0usize;

        let Ok(mut dir) = fs::read_dir(&self.root)
            .await
            .inspect_err(|e| warn!("disk cache: cannot read {}: {e}", self.root.display()))
        else {
            return;
        };
        while let Ok(Some(entry)) = dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(EXPIRY_EXT) {
                continue;
            }
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            let size = meta.len() as usize;
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            total_bytes += size;
            entries.push((name, size, mtime));
        }

        if total_bytes <= self.max_bytes {
            return;
        }

        // Sort oldest first for eviction
        entries.sort_by_key(|(_, _, mtime)| *mtime);

        for (key, size, _) in entries {
            if total_bytes <= self.max_bytes {
                break;
            }
            let _ = fs::remove_file(self.root.join(&key)).await;
            let _ = fs::remove_file(self.root.join(format!("{key}{EXPIRY_EXT}"))).await;
            total_bytes = total_bytes.saturating_sub(size);
        }
    }

    pub async fn read_raw(&self, key: &str) -> Option<Vec<u8>> {
        let file_uuid = key_to_uuid(key);
        fs::read(self.root.join(file_uuid.to_string())).await.ok()
    }

    async fn read_expiry(&self, file_uuid: &Uuid) -> Option<(u64, Option<String>)> {
        let path = self.root.join(format!("{file_uuid}{EXPIRY_EXT}"));
        let buf = fs::read(&path).await.ok()?;
        decode_expiry(&buf)
    }

    async fn write_expiry(&self, file_uuid: &Uuid, ttl: Duration, etag: Option<&str>) {
        let content = Self::build_expiry_content(ttl, etag);
        let path = self.root.join(format!("{file_uuid}{EXPIRY_EXT}"));
        if let Err(e) = fs::write(&path, &content).await {
            warn!("disk cache: failed to write expiry for {file_uuid}: {e}");
        }
    }

    fn build_expiry_content(ttl: Duration, etag: Option<&str>) -> Vec<u8> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        let expires_at = (now + ttl).as_secs();
        encode_expiry(expires_at, etag)
    }
}

/// Encode an expiry record: 8-byte BE timestamp + optional ETag string.
fn encode_expiry(expires_at_secs: u64, etag: Option<&str>) -> Vec<u8> {
    let mut content = expires_at_secs.to_be_bytes().to_vec();
    if let Some(etag) = etag {
        content.extend_from_slice(etag.as_bytes());
    }
    content
}

/// Decode an expiry record. Returns None if the buffer is too short.
fn decode_expiry(buf: &[u8]) -> Option<(u64, Option<String>)> {
    if buf.len() < 8 {
        return None;
    }
    let mut ts_bytes = [0u8; 8];
    ts_bytes.copy_from_slice(&buf[..8]);
    let ts = u64::from_be_bytes(ts_bytes);
    let etag = if buf.len() > 8 {
        String::from_utf8(buf[8..].to_vec()).ok()
    } else {
        None
    };
    Some((ts, etag))
}

fn is_expired(timestamp_secs: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    now > timestamp_secs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_roundtrip_with_etag() {
        let encoded = encode_expiry(1_700_000_000, Some("W/\"abc123\""));
        let (ts, etag) = decode_expiry(&encoded).expect("should decode");
        assert_eq!(ts, 1_700_000_000);
        assert_eq!(etag.as_deref(), Some("W/\"abc123\""));
    }

    #[test]
    fn expiry_roundtrip_without_etag() {
        let encoded = encode_expiry(42, None);
        let (ts, etag) = decode_expiry(&encoded).expect("should decode");
        assert_eq!(ts, 42);
        assert_eq!(etag, None);
    }

    #[test]
    fn decode_rejects_short_buffer() {
        assert!(decode_expiry(&[0u8; 4]).is_none());
        assert!(decode_expiry(&[]).is_none());
    }

    #[test]
    fn is_expired_boundaries() {
        assert!(is_expired(0), "epoch is always in the past");
        assert!(
            !is_expired(u64::MAX),
            "max timestamp is always in the future"
        );
    }

    #[test]
    fn key_to_uuid_is_deterministic() {
        assert_eq!(key_to_uuid("album_x.json"), key_to_uuid("album_x.json"));
        assert_ne!(key_to_uuid("album_x.json"), key_to_uuid("album_y.json"));
        assert_ne!(key_to_uuid("a"), key_to_uuid("A"));
    }

    /// Pins the namespace and algorithm: a change here invalidates every cache entry.
    #[test]
    fn key_to_uuid_matches_known_vector() {
        assert_eq!(
            key_to_uuid("album_4aawyAB9vmqN3uQ7FjRGTy.json").to_string(),
            "9ccb6477-8f9f-55d6-9ac6-4f900bae5ada"
        );
    }

    use std::sync::atomic::{AtomicU64, Ordering};

    const TEST_TTL: Duration = Duration::from_secs(300);

    struct TempDisk {
        cache: DiskCache,
    }

    impl std::ops::Deref for TempDisk {
        type Target = DiskCache;
        fn deref(&self) -> &DiskCache {
            &self.cache
        }
    }

    impl Drop for TempDisk {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.cache.root);
        }
    }

    fn temp_disk(default_ttl: Duration, max_bytes: u64) -> TempDisk {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let subdir = format!("riff-test/{}-{}", std::process::id(), n);
        TempDisk {
            cache: DiskCache::new(&subdir, max_bytes as usize, default_ttl),
        }
    }

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Total data file size (excluding `.expiry` sidecars).
    fn data_bytes(root: &std::path::Path) -> u64 {
        let mut total = 0;
        for entry in std::fs::read_dir(root).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(EXPIRY_EXT) {
                continue;
            }
            total += entry.metadata().unwrap().len();
        }
        total
    }

    #[tokio::test]
    async fn write_then_read_is_fresh() {
        let disk = temp_disk(TEST_TTL, 1 << 20);
        disk.write("k", b"hello", TEST_TTL, None).await;

        let entry = disk.read("k").await.expect("entry should exist");
        assert_eq!(&*entry.data, b"hello");
        assert!(matches!(entry.state, EntryState::Fresh));
    }

    #[tokio::test]
    async fn read_missing_is_none() {
        let disk = temp_disk(TEST_TTL, 1 << 20);
        assert!(disk.read("absent").await.is_none());
        assert!(disk.read_raw("absent").await.is_none());
    }

    #[tokio::test]
    async fn expired_entry_reads_stale_with_etag() {
        let disk = temp_disk(TEST_TTL, 1 << 20);
        disk.write("k", b"payload", TEST_TTL, Some("etag-1")).await;

        // Force expiry into the past while preserving the ETag.
        let expiry_path = disk.root.join(format!("{}{EXPIRY_EXT}", key_to_uuid("k")));
        std::fs::write(&expiry_path, encode_expiry(0, Some("etag-1"))).unwrap();

        let entry = disk.read("k").await.expect("entry should exist");
        assert_eq!(&*entry.data, b"payload");
        match entry.state {
            EntryState::Stale { etag } => assert_eq!(etag.as_deref(), Some("etag-1")),
            EntryState::Fresh => panic!("entry should be stale"),
        }

        assert_eq!(disk.read_raw("k").await.as_deref(), Some(&b"payload"[..]));
    }

    #[tokio::test]
    async fn invalidate_removes_data_and_expiry() {
        let disk = temp_disk(TEST_TTL, 1 << 20);
        disk.write("k", b"data", TEST_TTL, Some("e")).await;
        disk.invalidate("k").await;

        assert!(disk.read("k").await.is_none());
        assert!(!disk.root.join(key_to_uuid("k").to_string()).exists());
        assert!(!disk
            .root
            .join(format!("{}{EXPIRY_EXT}", key_to_uuid("k")))
            .exists());
    }

    #[tokio::test]
    async fn write_default_applies_default_ttl() {
        let disk = temp_disk(TEST_TTL, 1 << 20);
        let ttl = TEST_TTL.as_secs();
        let before = now_secs();
        disk.write_default("k", b"data", None).await;

        let (expires_at, _etag) = disk
            .read_expiry(&key_to_uuid("k"))
            .await
            .expect("expiry should exist");
        assert!(
            expires_at >= before + ttl - 10 && expires_at <= now_secs() + ttl + 10,
            "expiry {expires_at} should be roughly now + default_ttl (~{})",
            before + ttl
        );
    }

    #[tokio::test]
    async fn refresh_ttl_preserves_existing_etag() {
        let disk = temp_disk(TEST_TTL, 1 << 20);
        disk.write("k", b"data", Duration::from_secs(1), Some("keep-me"))
            .await;

        // Refresh with no ETag: the old ETag must survive.
        disk.refresh_ttl("k", Duration::from_secs(600), None).await;

        let (_ts, etag) = disk
            .read_expiry(&key_to_uuid("k"))
            .await
            .expect("expiry should exist");
        assert_eq!(etag.as_deref(), Some("keep-me"));
    }

    #[tokio::test]
    async fn clear_removes_everything() {
        let disk = temp_disk(TEST_TTL, 1 << 20);
        for i in 0..3 {
            disk.write(&format!("k{i}"), b"x", TEST_TTL, None).await;
        }
        disk.clear().await;
        assert_eq!(std::fs::read_dir(&disk.root).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn evict_to_budget_enforces_limit() {
        let disk = temp_disk(TEST_TTL, 250);
        for i in 0..5 {
            disk.write(&format!("k{i}"), &[0u8; 100], TEST_TTL, None)
                .await;
        }
        assert!(data_bytes(&disk.root) > 250, "precondition: over budget");

        disk.evict_to_budget().await;

        assert!(
            data_bytes(&disk.root) <= 250,
            "data bytes {} should be within budget 250",
            data_bytes(&disk.root)
        );
    }
}
