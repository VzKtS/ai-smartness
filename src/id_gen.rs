use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Longueur du hash projet (SHA-256 tronque)
pub const PROJECT_HASH_LEN: usize = 12;

/// Hash pur d'un chemin deja canonicalise (WASM-safe, pas d'I/O).
///
/// 1. SHA-256 du canonical_path
/// 2. Tronquer a PROJECT_HASH_LEN chars hex
///
/// Note: l'appelant DOIT canonicalize() le chemin avant d'appeler cette fonction.
/// Voir `crate::storage::path_utils::project_hash()` pour le wrapper avec canonicalize.
pub fn hash_path_string(canonical_path: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_path.as_bytes());
    let result = hasher.finalize();
    let hex = format!("{:x}", result);
    hex[..PROJECT_HASH_LEN].to_string()
}

/// Genere un ID unique pour un thread (UUID v4 hex, 32 chars)
pub fn thread_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Genere un ID unique pour un bridge (UUID v4 hex, 32 chars)
pub fn bridge_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Genere un ID unique pour un message (UUID v4 hex, 32 chars)
pub fn message_id() -> String {
    Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_path_string() {
        let hash = hash_path_string("/home/user/project");
        assert_eq!(hash.len(), PROJECT_HASH_LEN);
        // Deterministic
        assert_eq!(hash, hash_path_string("/home/user/project"));
        // Different paths give different hashes
        assert_ne!(hash, hash_path_string("/home/user/other"));
    }

    #[test]
    fn test_ids_are_unique() {
        let a = thread_id();
        let b = thread_id();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
    }
}
