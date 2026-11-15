//! Secrets at rest: the Strava client secret and OAuth tokens live in Windows
//! Credential Manager, not in the SQLite settings table.
//!
//! If the credential store is unavailable the app keeps working from the
//! settings table (and logs a warning). Secret values are never logged: every
//! log line here goes through `warn`, which only takes the key name and the
//! error text.

use crate::persistence;
use tauri::AppHandle;

/// Credential Manager target prefix: the bundle identifier.
const SERVICE: &str = "com.freshbreweddesign.neonstrider";

/// A secret the app stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    /// The Strava API application's client secret.
    ClientSecret,
    /// The Strava token set (JSON).
    Auth,
}

impl Secret {
    pub const ALL: [Secret; 2] = [Secret::ClientSecret, Secret::Auth];

    /// The Credential Manager entry name, which is also the settings key of the
    /// plaintext fallback copy. Existing installs read these: never rename them.
    pub const fn key(self) -> &'static str {
        match self {
            Secret::ClientSecret => "stravaClientSecret",
            Secret::Auth => "stravaAuth",
        }
    }
}

/// Settings keys whose values must never sit in the database once the
/// credential store works. The generic settings commands refuse them.
pub fn is_protected_key(key: &str) -> bool {
    Secret::ALL.iter().any(|s| s.key() == key)
}

/// Minimal secret store, so the logic can be tested without Credential Manager.
pub trait SecretStore {
    /// `Ok(None)` when nothing is stored under `key`.
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, value: &str) -> Result<(), String>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// Windows Credential Manager through the `keyring` crate.
pub struct KeyringStore;

fn entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, key).map_err(|e| e.to_string())
}

impl SecretStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        match entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        entry(key)?.set_password(value).map_err(|e| e.to_string())
    }

    fn delete(&self, key: &str) -> Result<(), String> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// What the credential store said when asked for a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreRead {
    Value(String),
    Missing,
    Unavailable,
}

/// What startup does with one secret's plaintext database copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStep {
    /// No plaintext copy in the database.
    Nothing,
    /// Write the database value to the store, read it back, and only then
    /// delete it from the database.
    CopyVerifyThenClear,
    /// The store already holds this exact value; just drop the database copy.
    ClearDatabase,
    /// Store unusable: leave the database value alone and keep using it.
    KeepInDatabase,
}

/// A database value that is still present is always the newest one: a value
/// is only ever written there when the store failed, and every successful
/// store write deletes the database copy. So a differing store value is stale.
fn decide_migration(db: Option<&str>, store: &StoreRead) -> MigrationStep {
    let Some(db) = db.filter(|v| !v.is_empty()) else {
        return MigrationStep::Nothing;
    };
    match store {
        StoreRead::Unavailable => MigrationStep::KeepInDatabase,
        StoreRead::Value(v) if v == db => MigrationStep::ClearDatabase,
        StoreRead::Value(_) | StoreRead::Missing => MigrationStep::CopyVerifyThenClear,
    }
}

fn read_store(store: &dyn SecretStore, key: &str) -> StoreRead {
    match store.get(key) {
        Ok(Some(v)) => StoreRead::Value(v),
        Ok(None) => StoreRead::Missing,
        Err(_) => StoreRead::Unavailable,
    }
}

/// The plaintext fallback copy of a secret in the settings table.
pub trait DbSlot {
    fn read(&self, key: &str) -> Option<String>;
    fn write(&self, key: &str, value: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

struct SettingsTable<'a>(&'a AppHandle);

impl DbSlot for SettingsTable<'_> {
    fn read(&self, key: &str) -> Option<String> {
        persistence::read_setting(self.0, key)
    }
    fn write(&self, key: &str, value: &str) -> Result<(), String> {
        persistence::write_setting(self.0, key, value)
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        persistence::delete_setting(self.0, key)
    }
}

/// The only logging in this module: the key name and the error text, never a value.
fn warn(what: &str, key: &str, err: &str) {
    log::warn!("{} for '{}': {}", what, key, err);
}

/// Write then read back; Ok only if the store returns exactly `value`.
fn write_verified(store: &dyn SecretStore, key: &str, value: &str) -> Result<(), String> {
    store.set(key, value)?;
    match store.get(key)? {
        Some(back) if back == value => Ok(()),
        _ => Err("read-back did not match".to_string()),
    }
}

fn get_with(store: &dyn SecretStore, db: &dyn DbSlot, key: &str) -> Option<String> {
    // A database copy only exists while it is the newest value (see
    // `decide_migration`), so it wins over whatever the store holds.
    if let Some(v) = db.read(key).filter(|v| !v.is_empty()) {
        return Some(v);
    }
    match store.get(key) {
        Ok(v) => v,
        Err(e) => {
            warn("credential store unavailable", key, &e);
            None
        }
    }
}

fn set_with(
    store: &dyn SecretStore,
    db: &dyn DbSlot,
    key: &str,
    value: &str,
) -> Result<(), String> {
    match write_verified(store, key, value) {
        Ok(()) => {
            if let Err(e) = db.delete(key) {
                warn("could not remove the database copy", key, &e);
            }
            Ok(())
        }
        Err(e) => {
            warn(
                "credential store write failed, keeping it in the database",
                key,
                &e,
            );
            db.write(key, value)
        }
    }
}

fn delete_with(store: &dyn SecretStore, db: &dyn DbSlot, key: &str) {
    if let Err(e) = store.delete(key) {
        warn("could not delete from the credential store", key, &e);
    }
    if let Err(e) = db.delete(key) {
        warn("could not delete from the database", key, &e);
    }
}

fn migrate_with(store: &dyn SecretStore, db: &dyn DbSlot, key: &str) {
    let db_value = db.read(key);
    let step = decide_migration(db_value.as_deref(), &read_store(store, key));
    let clear_db = |why: &str| {
        if let Err(e) = db.delete(key) {
            warn(why, key, &e);
        }
    };
    match step {
        MigrationStep::Nothing => {}
        MigrationStep::KeepInDatabase => {
            warn(
                "credential store unavailable, secret stays in the database",
                key,
                "-",
            );
        }
        MigrationStep::ClearDatabase => clear_db("could not remove the database copy"),
        MigrationStep::CopyVerifyThenClear => {
            let value = db_value.unwrap_or_default();
            match write_verified(store, key, &value) {
                Ok(()) => clear_db("migrated but could not remove the database copy"),
                Err(e) => warn("migration failed, secret stays in the database", key, &e),
            }
        }
    }
}

/// The secret's value: the database fallback copy if there is one, else the
/// credential store.
pub fn get(app: &AppHandle, secret: Secret) -> Option<String> {
    get_with(&KeyringStore, &SettingsTable(app), secret.key())
}

/// Store the secret in Credential Manager (verified), or in the settings table
/// when the store does not work.
pub fn set(app: &AppHandle, secret: Secret, value: &str) -> Result<(), String> {
    set_with(&KeyringStore, &SettingsTable(app), secret.key(), value)
}

/// Remove the secret from both places. Failures are logged, not returned.
pub fn delete(app: &AppHandle, secret: Secret) {
    delete_with(&KeyringStore, &SettingsTable(app), secret.key())
}

/// Move any plaintext secrets out of the database. Safe to run every startup.
pub fn migrate(app: &AppHandle) {
    for secret in Secret::ALL {
        migrate_with(&KeyringStore, &SettingsTable(app), secret.key());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemStore {
        map: RefCell<HashMap<String, String>>,
        broken: bool,
        /// Accepts writes but reads back something else.
        lossy: bool,
    }
    impl SecretStore for MemStore {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            if self.broken {
                return Err("down".into());
            }
            Ok(self.map.borrow().get(key).map(|v| {
                if self.lossy {
                    format!("{v}x")
                } else {
                    v.clone()
                }
            }))
        }
        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            if self.broken {
                return Err("down".into());
            }
            self.map.borrow_mut().insert(key.into(), value.into());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), String> {
            if self.broken {
                return Err("down".into());
            }
            self.map.borrow_mut().remove(key);
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemDb(RefCell<HashMap<String, String>>);
    impl DbSlot for MemDb {
        fn read(&self, key: &str) -> Option<String> {
            self.0.borrow().get(key).cloned()
        }
        fn write(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.borrow_mut().insert(key.into(), value.into());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), String> {
            self.0.borrow_mut().remove(key);
            Ok(())
        }
    }

    const K: &str = "stravaClientSecret";

    #[test]
    fn decision_table() {
        use MigrationStep::*;
        let v = |s: &str| StoreRead::Value(s.to_string());
        assert_eq!(decide_migration(None, &StoreRead::Missing), Nothing);
        assert_eq!(decide_migration(Some(""), &StoreRead::Missing), Nothing);
        assert_eq!(decide_migration(None, &v("a")), Nothing);
        assert_eq!(
            decide_migration(Some("a"), &StoreRead::Missing),
            CopyVerifyThenClear
        );
        assert_eq!(decide_migration(Some("a"), &v("a")), ClearDatabase);
        assert_eq!(decide_migration(Some("a"), &v("old")), CopyVerifyThenClear);
        assert_eq!(
            decide_migration(Some("a"), &StoreRead::Unavailable),
            KeepInDatabase
        );
        assert_eq!(decide_migration(None, &StoreRead::Unavailable), Nothing);
    }

    #[test]
    fn migrates_and_clears_database() {
        let (store, db) = (MemStore::default(), MemDb::default());
        db.write(K, "s3cret").unwrap();
        migrate_with(&store, &db, K);
        assert_eq!(store.get(K).unwrap().as_deref(), Some("s3cret"));
        assert_eq!(db.read(K), None);
    }

    #[test]
    fn unavailable_store_keeps_database_value_and_still_reads() {
        let (store, db) = (
            MemStore {
                broken: true,
                ..Default::default()
            },
            MemDb::default(),
        );
        db.write(K, "s3cret").unwrap();
        migrate_with(&store, &db, K);
        assert_eq!(db.read(K).as_deref(), Some("s3cret"));
        assert_eq!(get_with(&store, &db, K).as_deref(), Some("s3cret"));
    }

    #[test]
    fn failed_verification_keeps_database_value() {
        let (store, db) = (
            MemStore {
                lossy: true,
                ..Default::default()
            },
            MemDb::default(),
        );
        db.write(K, "s3cret").unwrap();
        migrate_with(&store, &db, K);
        assert_eq!(db.read(K).as_deref(), Some("s3cret"));
    }

    #[test]
    fn set_goes_to_store_and_clears_database() {
        let (store, db) = (MemStore::default(), MemDb::default());
        db.write(K, "old").unwrap();
        set_with(&store, &db, K, "new").unwrap();
        assert_eq!(get_with(&store, &db, K).as_deref(), Some("new"));
        assert_eq!(db.read(K), None);
    }

    #[test]
    fn set_falls_back_to_database_when_store_is_down() {
        let (store, db) = (
            MemStore {
                broken: true,
                ..Default::default()
            },
            MemDb::default(),
        );
        set_with(&store, &db, K, "new").unwrap();
        assert_eq!(db.read(K).as_deref(), Some("new"));
        assert_eq!(get_with(&store, &db, K).as_deref(), Some("new"));
    }

    #[test]
    fn delete_clears_both() {
        let (store, db) = (MemStore::default(), MemDb::default());
        store.set(K, "a").unwrap();
        db.write(K, "b").unwrap();
        delete_with(&store, &db, K);
        assert_eq!(get_with(&store, &db, K), None);
    }

    #[test]
    fn protected_keys() {
        assert!(is_protected_key("stravaAuth"));
        assert!(is_protected_key("stravaClientSecret"));
        assert!(!is_protected_key("stravaClientId"));
    }

    #[test]
    fn entry_names_are_fixed() {
        // Existing installs have their secrets under these names.
        assert_eq!(Secret::ClientSecret.key(), "stravaClientSecret");
        assert_eq!(Secret::Auth.key(), "stravaAuth");
    }
}
