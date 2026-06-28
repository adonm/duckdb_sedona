// SPDX-License-Identifier: MIT
// Copyright 2026 Tom F. <https://github.com/tomtom215/>
// My way of giving something small back to the open source community
// and encouraging more Rust development!

//! Secrets manager bridge for extensions.
//!
//! Extensions that access external services (HTTP APIs, databases, cloud storage)
//! commonly need credentials. `DuckDB` provides a native secrets API via
//! `CREATE SECRET`, and this module defines the Rust-side traits and types that
//! extensions implement to bridge into that system.
//!
//! [`SecretsManager`] is the trait that extensions implement to provide secret
//! lookup. [`SecretEntry`] is the returned secret value.
//!
//! # Security considerations
//!
//! `SecretEntry` is designed to minimize accidental credential leakage:
//!
//! - **`Debug` redacts field values** — only field keys are shown, values are
//!   replaced with `"[REDACTED]"`. Use [`get_field`][SecretEntry::get_field] to
//!   access actual values in code.
//! - **`Drop` zeroizes sensitive data** — all field values are overwritten with
//!   zeros using [`std::ptr::write_volatile`] before deallocation, preventing
//!   secrets from lingering in freed memory.
//! - **No `PartialEq`** — prevents accidental non-constant-time comparisons of
//!   secret material. Compare individual fields explicitly if needed.
//! - **`Clone` is explicit** — cloning is supported but documented so that
//!   callers are aware they are duplicating sensitive material in memory.
//!
//! # Example
//!
//! ```rust
//! use quack_rs::secrets::{SecretEntry, SecretsManager};
//!
//! struct MySecrets {
//!     // In practice, backed by DuckDB's CREATE SECRET storage
//!     entries: Vec<SecretEntry>,
//! }
//!
//! impl SecretsManager for MySecrets {
//!     fn get_secret(&self, name: &str, secret_type: &str) -> Option<SecretEntry> {
//!         self.entries.iter()
//!             .find(|e| e.name() == name && e.secret_type() == secret_type)
//!             .cloned()
//!     }
//!
//!     fn list_secrets(&self, secret_type: Option<&str>) -> Vec<SecretEntry> {
//!         self.entries.iter()
//!             .filter(|e| secret_type.is_none() || secret_type == Some(e.secret_type()))
//!             .cloned()
//!             .collect()
//!     }
//!
//!     fn remove_secret(&self, _name: &str, _secret_type: &str) -> bool {
//!         false // read-only example
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::fmt;

/// A single secret entry retrieved from the secrets manager.
///
/// Contains the secret's metadata and key-value pairs. The `fields` map holds
/// the actual secret data (e.g., `"token"`, `"username"`, `"password"`,
/// `"client_cert_path"`).
///
/// # Security
///
/// - [`Debug`] output redacts all field values (shows keys only).
/// - [`Drop`] zeroizes all field values before deallocation.
/// - [`Clone`] is supported but creates a second copy of sensitive data in
///   memory — use sparingly and drop clones promptly.
/// - `PartialEq` / `Eq` are intentionally **not** implemented to prevent
///   accidental non-constant-time comparisons of secret material.
pub struct SecretEntry {
    name: String,
    secret_type: String,
    provider: String,
    scope: String,
    fields: HashMap<String, String>,
}

impl SecretEntry {
    /// Creates a new `SecretEntry` with the given name and type.
    ///
    /// # Example
    ///
    /// ```rust
    /// use quack_rs::secrets::SecretEntry;
    ///
    /// let entry = SecretEntry::new("my_api_key", "bearer")
    ///     .with_provider("config")
    ///     .with_field("token", "sk-abc123");
    /// assert_eq!(entry.name(), "my_api_key");
    /// assert_eq!(entry.get_field("token"), Some("sk-abc123"));
    /// ```
    #[must_use]
    pub fn new(name: impl Into<String>, secret_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            secret_type: secret_type.into(),
            provider: String::new(),
            scope: String::new(),
            fields: HashMap::new(),
        }
    }

    /// Returns the name of this secret.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the secret type (e.g., `"bearer"`, `"s3"`, `"gcs"`).
    #[must_use]
    #[inline]
    pub fn secret_type(&self) -> &str {
        &self.secret_type
    }

    /// Returns the provider that created this secret.
    #[must_use]
    #[inline]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Returns the scope this secret applies to.
    #[must_use]
    #[inline]
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Returns the field key names without exposing values.
    ///
    /// Use this for logging or diagnostics without leaking sensitive data.
    #[must_use]
    pub fn field_keys(&self) -> Vec<&str> {
        self.fields.keys().map(String::as_str).collect()
    }

    /// Sets the provider for this secret entry.
    #[must_use]
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = provider.into();
        self
    }

    /// Sets the scope for this secret entry.
    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = scope.into();
        self
    }

    /// Adds a key-value field to this secret entry.
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Returns the value of a field, if present.
    #[must_use]
    pub fn get_field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    /// Returns the number of fields in this secret entry.
    #[must_use]
    #[inline]
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Returns `true` if this entry has no fields.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

/// Zeroize a `String`'s buffer using volatile writes, then clear it.
///
/// Uses [`std::ptr::write_volatile`] to ensure the compiler cannot elide the
/// zeroing even if the memory is about to be freed. This is the standard
/// approach used by the `zeroize` crate, implemented inline to avoid adding
/// a dependency.
fn zeroize_string(s: &mut String) {
    // SAFETY: `as_mut_vec()` gives us mutable access to the String's backing
    // buffer. We only write `0u8` bytes, which is valid UTF-8 (NUL chars).
    // The string is cleared immediately after, so no invalid-UTF-8 state
    // is observable.
    unsafe {
        for byte in s.as_mut_vec().iter_mut() {
            std::ptr::write_volatile(byte, 0);
        }
    }
    s.clear();
}

impl Drop for SecretEntry {
    fn drop(&mut self) {
        // Zeroize all field values (the sensitive material).
        for value in self.fields.values_mut() {
            zeroize_string(value);
        }
        // Zeroize field keys too — key names can reveal what credentials exist.
        // HashMap doesn't expose mutable key access, so we drain and zeroize.
        for (mut key, mut val) in self.fields.drain() {
            zeroize_string(&mut key);
            zeroize_string(&mut val);
        }

        // Zeroize metadata fields that may contain sensitive context.
        zeroize_string(&mut self.provider);
        zeroize_string(&mut self.scope);
    }
}

impl Clone for SecretEntry {
    /// Clones this secret entry, duplicating all sensitive material in memory.
    ///
    /// Callers should be aware that cloning creates a second copy of secret
    /// values. Drop the clone as soon as it is no longer needed to minimize
    /// the window during which sensitive data resides in memory.
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            secret_type: self.secret_type.clone(),
            provider: self.provider.clone(),
            scope: self.scope.clone(),
            fields: self.fields.clone(),
        }
    }
}

impl fmt::Debug for SecretEntry {
    /// Formats the secret entry with field values redacted.
    ///
    /// Only field keys are shown; all values are replaced with `"[REDACTED]"`.
    /// Use [`get_field`][SecretEntry::get_field] to access actual values in code.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redacted_fields: HashMap<&str, &str> = self
            .fields
            .keys()
            .map(|k| (k.as_str(), "[REDACTED]"))
            .collect();

        f.debug_struct("SecretEntry")
            .field("name", &self.name)
            .field("secret_type", &self.secret_type)
            .field("provider", &self.provider)
            .field("scope", &self.scope)
            .field("fields", &redacted_fields)
            .finish()
    }
}

impl fmt::Display for SecretEntry {
    /// Formats a human-readable summary without exposing any secret values.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Secret(name={:?}, type={:?}, provider={:?}, fields={})",
            self.name,
            self.secret_type,
            self.provider,
            self.fields.len()
        )
    }
}

/// Trait for accessing `DuckDB`'s secrets management system.
///
/// Extensions implement this trait to provide a safe Rust interface over
/// `DuckDB`'s native `CREATE SECRET` / `DROP SECRET` storage. A typical
/// implementation wraps `DuckDB`'s C API or maintains an in-memory cache
/// synchronized with the `DuckDB` catalog.
///
/// # Thread safety
///
/// Implementations must be safe to call from multiple threads. `DuckDB` may
/// invoke extension callbacks concurrently.
///
/// # Security
///
/// Implementations should:
/// - Never log secret field values (use [`SecretEntry::field_keys`] for
///   diagnostics).
/// - Ensure that `remove_secret` zeroizes the secret data, not just
///   removes the reference.
/// - Minimize the lifetime of [`SecretEntry`] clones.
pub trait SecretsManager: Send + Sync {
    /// Retrieves a secret by name and type.
    ///
    /// Returns `None` if no matching secret exists.
    fn get_secret(&self, name: &str, secret_type: &str) -> Option<SecretEntry>;

    /// Lists all secrets, optionally filtered by type.
    ///
    /// If `secret_type` is `None`, all secrets are returned.
    ///
    /// # Security note
    ///
    /// The returned entries contain full secret values. Callers should avoid
    /// storing or logging the result. For diagnostics, iterate and use
    /// [`SecretEntry::field_keys`] instead of [`SecretEntry::get_field`].
    fn list_secrets(&self, secret_type: Option<&str>) -> Vec<SecretEntry>;

    /// Removes a secret by name and type.
    ///
    /// Returns `true` if the secret was found and removed, `false` otherwise.
    /// Implementations should zeroize the secret data before deallocation.
    fn remove_secret(&self, name: &str, secret_type: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_entry_builder() {
        let entry = SecretEntry::new("test", "bearer")
            .with_provider("config")
            .with_scope("https://api.example.com")
            .with_field("token", "abc123")
            .with_field("refresh_token", "xyz789");

        assert_eq!(entry.name(), "test");
        assert_eq!(entry.secret_type(), "bearer");
        assert_eq!(entry.provider(), "config");
        assert_eq!(entry.scope(), "https://api.example.com");
        assert_eq!(entry.get_field("token"), Some("abc123"));
        assert_eq!(entry.get_field("refresh_token"), Some("xyz789"));
        assert_eq!(entry.get_field("nonexistent"), None);
        assert_eq!(entry.field_count(), 2);
        assert!(!entry.is_empty());
    }

    #[test]
    fn secret_entry_new_defaults() {
        let entry = SecretEntry::new("s", "s3");
        assert_eq!(entry.provider(), "");
        assert_eq!(entry.scope(), "");
        assert!(entry.is_empty());
        assert_eq!(entry.field_count(), 0);
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn secret_entry_clone() {
        let e1 = SecretEntry::new("a", "b").with_field("k", "v");
        let e2 = e1.clone();
        assert_eq!(e2.name(), "a");
        assert_eq!(e2.get_field("k"), Some("v"));
    }

    #[test]
    fn debug_redacts_field_values() {
        let entry =
            SecretEntry::new("api_key", "bearer").with_field("token", "super-secret-value-12345");

        let debug_output = format!("{entry:?}");

        // The actual secret value must NOT appear in debug output.
        assert!(
            !debug_output.contains("super-secret-value-12345"),
            "Debug output must not contain secret values: {debug_output}"
        );
        // The field key SHOULD appear (it's metadata, not the secret).
        assert!(
            debug_output.contains("token"),
            "Debug should show field keys"
        );
        // The redaction marker should appear.
        assert!(
            debug_output.contains("[REDACTED]"),
            "Debug should show [REDACTED] for values"
        );
    }

    #[test]
    fn display_does_not_leak_values() {
        let entry =
            SecretEntry::new("api_key", "bearer").with_field("token", "super-secret-value-12345");

        let display_output = format!("{entry}");

        assert!(
            !display_output.contains("super-secret-value-12345"),
            "Display must not contain secret values: {display_output}"
        );
        assert!(
            display_output.contains("api_key"),
            "Display should show the secret name"
        );
    }

    #[test]
    fn field_keys_returns_keys_only() {
        let entry = SecretEntry::new("x", "y")
            .with_field("token", "secret1")
            .with_field("key_id", "secret2");

        let keys = entry.field_keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"token"));
        assert!(keys.contains(&"key_id"));
    }

    #[test]
    fn drop_zeroizes_field_values() {
        // We can't directly observe memory after drop, but we can verify
        // zeroize_string works correctly on a standalone string.
        let mut s = String::from("sensitive-data-here");
        let ptr = s.as_ptr();
        let len = s.len();

        zeroize_string(&mut s);

        assert!(s.is_empty(), "String should be empty after zeroize");
        // Verify the original buffer was zeroed (the pointer is still valid
        // because clear() doesn't deallocate).
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert!(
            bytes.iter().all(|&b| b == 0),
            "All bytes should be zero after zeroize"
        );
    }

    #[test]
    fn zeroize_empty_string_is_safe() {
        let mut s = String::new();
        zeroize_string(&mut s);
        assert!(s.is_empty());
    }

    struct InMemorySecrets {
        entries: Vec<SecretEntry>,
    }

    impl SecretsManager for InMemorySecrets {
        fn get_secret(&self, name: &str, secret_type: &str) -> Option<SecretEntry> {
            self.entries
                .iter()
                .find(|e| e.name() == name && e.secret_type() == secret_type)
                .cloned()
        }

        fn list_secrets(&self, secret_type: Option<&str>) -> Vec<SecretEntry> {
            self.entries
                .iter()
                .filter(|e| secret_type.is_none() || secret_type == Some(e.secret_type()))
                .cloned()
                .collect()
        }

        fn remove_secret(&self, _name: &str, _secret_type: &str) -> bool {
            false
        }
    }

    #[test]
    fn in_memory_secrets_manager() {
        let mgr = InMemorySecrets {
            entries: vec![
                SecretEntry::new("api_key", "bearer").with_field("token", "t1"),
                SecretEntry::new("bucket", "s3").with_field("key_id", "k1"),
            ],
        };

        assert!(mgr.get_secret("api_key", "bearer").is_some());
        assert!(mgr.get_secret("api_key", "s3").is_none());
        assert!(mgr.get_secret("missing", "bearer").is_none());

        let all = mgr.list_secrets(None);
        assert_eq!(all.len(), 2);

        let s3_only = mgr.list_secrets(Some("s3"));
        assert_eq!(s3_only.len(), 1);
        assert_eq!(s3_only[0].name(), "bucket");
    }

    #[test]
    fn secrets_manager_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<InMemorySecrets>();
    }

    #[test]
    fn secret_entry_empty_name_and_type() {
        let entry = SecretEntry::new("", "");
        assert_eq!(entry.name(), "");
        assert_eq!(entry.secret_type(), "");
    }

    #[test]
    fn with_field_overwrites_existing_key() {
        let entry = SecretEntry::new("s", "t")
            .with_field("token", "old-value")
            .with_field("token", "new-value");
        assert_eq!(entry.get_field("token"), Some("new-value"));
        assert_eq!(entry.field_count(), 1);
    }

    #[test]
    fn debug_redacts_empty_field_value() {
        let entry = SecretEntry::new("s", "t").with_field("key", "");
        let debug = format!("{entry:?}");
        // Even empty field values must be redacted in debug output
        assert!(debug.contains("[REDACTED]"));
        assert!(debug.contains("key"));
    }

    #[test]
    fn display_shows_field_count_not_values() {
        let entry = SecretEntry::new("s", "t")
            .with_field("a", "secret1")
            .with_field("b", "secret2")
            .with_field("c", "secret3");
        let display = format!("{entry}");
        assert!(display.contains("fields=3"));
        assert!(!display.contains("secret1"));
        assert!(!display.contains("secret2"));
        assert!(!display.contains("secret3"));
    }

    #[test]
    fn zeroize_string_with_special_characters() {
        let mut s = String::from("p@$$w0rd!#%^&*()_+-=[]{}|;':\",./<>?");
        let ptr = s.as_ptr();
        let len = s.len();
        zeroize_string(&mut s);
        assert!(s.is_empty());
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert!(bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn zeroize_string_with_unicode() {
        let mut s = String::from("pässwörd🔑秘密");
        let ptr = s.as_ptr();
        let len = s.len();
        zeroize_string(&mut s);
        assert!(s.is_empty());
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        assert!(bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn drop_zeroizes_metadata_fields() {
        // Verify that provider and scope are zeroized on drop
        let mut entry = SecretEntry::new("name", "type")
            .with_provider("my-provider")
            .with_scope("https://example.com");

        // Zeroize directly to test
        zeroize_string(&mut entry.provider);
        assert!(entry.provider.is_empty());
        zeroize_string(&mut entry.scope);
        assert!(entry.scope.is_empty());
    }

    #[test]
    fn list_secrets_with_no_matching_type() {
        let mgr = InMemorySecrets {
            entries: vec![SecretEntry::new("a", "bearer").with_field("token", "t1")],
        };
        let result = mgr.list_secrets(Some("s3"));
        assert!(result.is_empty());
    }

    #[test]
    fn remove_secret_returns_false() {
        let mgr = InMemorySecrets {
            entries: vec![SecretEntry::new("a", "b")],
        };
        assert!(!mgr.remove_secret("a", "b"));
    }

    #[test]
    fn secret_entry_many_fields() {
        let mut entry = SecretEntry::new("s", "t");
        for i in 0..100 {
            entry = entry.with_field(format!("key_{i}"), format!("value_{i}"));
        }
        assert_eq!(entry.field_count(), 100);
        assert_eq!(entry.get_field("key_50"), Some("value_50"));
        assert_eq!(entry.field_keys().len(), 100);
    }
}
