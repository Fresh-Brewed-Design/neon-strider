//! The dashboard's starting settings. `cargo test` writes `AppConfig::default()`
//! to `src/ipc/bindings/DEFAULT_APP_CONFIG.ts` as `DEFAULT_APP_CONFIG`, so the
//! settings the dashboard shows before Rust's saved ones arrive are Rust's
//! defaults instead of a copy, and CI fails if the file falls behind a change
//! to `AppConfig::default()`.

use crate::persistence::AppConfig;

/// The generated file's text: `DEFAULT_APP_CONFIG` typed by the ts-rs binding.
pub fn typescript() -> String {
    crate::limits::constant_module(
        "defaults.rs",
        "AppConfig",
        "DEFAULT_APP_CONFIG",
        &AppConfig::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes the TypeScript constant beside the ts-rs bindings; CI clears the
    /// folder, runs the tests and fails on a diff.
    #[test]
    fn export_default_app_config() {
        crate::limits::write_binding("DEFAULT_APP_CONFIG.ts", &typescript());
    }

    /// The exported object reads back as `AppConfig::default()`, and names
    /// every field, so none falls back to a serde default on the way.
    #[test]
    fn export_matches_app_config_default() {
        let text = typescript();
        let start = text.find("= {").expect("object start") + 2;
        let end = text.rfind("};").expect("object end") + 1;
        let object = &text[start..end];
        let read: AppConfig = serde_json::from_str(object).expect("valid JSON");
        assert_eq!(read, AppConfig::default());
        let written: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(object).unwrap();
        let fields = serde_json::to_value(AppConfig::default()).unwrap();
        let fields = fields.as_object().unwrap();
        assert!(written.keys().eq(fields.keys()), "{object}");
    }
}
