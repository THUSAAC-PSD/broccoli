use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::StorageError;

/// A validated SHA-256 content hash.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Compute the SHA-256 hash of the given data.
    pub fn compute(data: &[u8]) -> Self {
        let hash = Sha256::digest(data);
        Self(hash.into())
    }

    /// Construct from raw SHA-256 bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Parse a hex-encoded content hash string.
    pub fn from_hex(s: &str) -> Result<Self, StorageError> {
        if s.len() != 64 {
            return Err(StorageError::InvalidHash(format!(
                "expected 64 hex characters, got {}",
                s.len()
            )));
        }

        let bytes =
            hex::decode(s).map_err(|e| StorageError::InvalidHash(format!("invalid hex: {e}")))?;

        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| StorageError::InvalidHash("decoded to wrong length".into()))?;

        Ok(Self(arr))
    }

    /// Return the hash as a 64-character lowercase hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Return the raw 32-byte hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Return the first 2 hex characters (shard prefix for filesystem layout).
    pub fn shard_prefix(&self) -> String {
        hex::encode(&self.0[..1])
    }

    /// Return the remaining 62 hex characters (filename within shard).
    pub fn shard_suffix(&self) -> String {
        hex::encode(&self.0[1..])
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Serialize for ContentHash {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_is_deterministic() {
        let data = b"hello world";
        let h1 = ContentHash::compute(data);
        let h2 = ContentHash::compute(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_differs_for_different_data() {
        let h1 = ContentHash::compute(b"hello");
        let h2 = ContentHash::compute(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hex_round_trip() {
        let original = ContentHash::compute(b"test data");
        let hex_str = original.to_hex();
        let parsed = ContentHash::from_hex(&hex_str).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_hex_rejects_non_hex() {
        let bad = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        assert!(ContentHash::from_hex(bad).is_err());
    }

    #[test]
    fn shard_prefix_and_suffix() {
        let hash = ContentHash::compute(b"test");
        let hex = hash.to_hex();
        assert_eq!(hash.shard_prefix(), &hex[..2]);
        assert_eq!(hash.shard_suffix(), &hex[2..]);
    }

    #[test]
    fn display_matches_to_hex() {
        let hash = ContentHash::compute(b"display test");
        assert_eq!(format!("{hash}"), hash.to_hex());
    }

    #[test]
    fn serde_round_trip() {
        let hash = ContentHash::compute(b"serde test");
        let json = serde_json::to_string(&hash).unwrap();
        let parsed: ContentHash = serde_json::from_str(&json).unwrap();
        assert_eq!(hash, parsed);
    }
}
