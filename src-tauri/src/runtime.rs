use std::fs;

use crate::logging::DiagnosticLogger;
use crate::state_dir::StateDir;

const RUNTIME_HEADER: &str = "(() => {\n  if (typeof document === \"undefined\") return;\n\n";
const RUNTIME_FOOTER: &str = "\n})();\n";

include!(concat!(env!("OUT_DIR"), "/runtime_modules.rs"));

const BUILD_DATE_PLACEHOLDER_LITERAL: &str = "\"__CODEX_HELPER_BUILD_DATE__\"";
const VERSION_PLACEHOLDER_LITERAL: &str = "\"__CODEX_HELPER_VERSION__\"";

fn helper_build_date() -> &'static str {
    let date = env!("CODEX_HELPER_BUILD_DATE");
    if date.is_empty() {
        panic!("CODEX_HELPER_BUILD_DATE was empty at compile time");
    }
    date
}

fn helper_version() -> &'static str {
    let version = env!("CARGO_PKG_VERSION");
    if version.is_empty() {
        panic!("CARGO_PKG_VERSION was empty at compile time");
    }
    version
}

fn helper_build_date_literal() -> String {
    serde_json::to_string(helper_build_date()).expect("serialize helper build date")
}

fn helper_version_literal() -> String {
    serde_json::to_string(helper_version()).expect("serialize helper version")
}

fn bundle_module_sources(sources: &[&str], kind: &str) -> String {
    let body = sources.join("\n\n");
    if !body.contains(BUILD_DATE_PLACEHOLDER_LITERAL) {
        panic!("{kind} bundle is missing the helper build date placeholder");
    }
    if !body.contains(VERSION_PLACEHOLDER_LITERAL) {
        panic!("{kind} bundle is missing the helper version placeholder");
    }
    let body = body
        .replace(BUILD_DATE_PLACEHOLDER_LITERAL, &helper_build_date_literal())
        .replace(VERSION_PLACEHOLDER_LITERAL, &helper_version_literal());
    format!("{RUNTIME_HEADER}{body}{RUNTIME_FOOTER}")
}

fn bundled_runtime() -> String {
    bundle_module_sources(RUNTIME_MODULE_SOURCES, "runtime")
}

pub fn build_settings_runtime() -> String {
    bundle_module_sources(SETTINGS_MODULE_SOURCES, "settings")
}

pub fn build_runtime_bundle(
    state_dir: &StateDir,
    logger: &DiagnosticLogger,
) -> anyhow::Result<Vec<String>> {
    let mut scripts = vec![bundled_runtime()];
    scripts.extend(
        STANDALONE_RUNTIME_SCRIPTS
            .iter()
            .map(|source| source.to_string()),
    );
    if !state_dir.scripts_dir.exists() {
        return Ok(scripts);
    }
    let mut user_script_paths = Vec::new();
    for entry in fs::read_dir(&state_dir.scripts_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("js") {
            user_script_paths.push(path);
        }
    }
    user_script_paths.sort();
    for path in user_script_paths {
        match fs::read_to_string(&path) {
            Ok(source) => scripts.push(source),
            Err(error) => {
                logger.append(
                    "runtime.user_script_read_failed",
                    serde_json::json!({
                        "path": path,
                        "error": error.to_string(),
                    }),
                )?;
            }
        }
    }
    Ok(scripts)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::logging::DiagnosticLogger;
    use crate::state_dir::StateDir;

    #[test]
    fn runtime_bundle_orders_builtin_then_user_scripts() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let state_dir =
            StateDir::init_at(temp_dir.path().join(".codex-helper")).expect("state dir");
        fs::write(state_dir.scripts_dir.join("b.js"), "window.b = true;").expect("script b");
        fs::write(state_dir.scripts_dir.join("a.js"), "window.a = true;").expect("script a");
        let logger = DiagnosticLogger::new(state_dir.logs_dir.clone());

        let bundle = build_runtime_bundle(&state_dir, &logger).expect("bundle");

        assert!(bundle[0].contains("Codex Helper") || bundle[0].contains("codex-helper"));
        assert!(
            !bundle[0].contains("zed_menu_item_injected"),
            "ChatGPT injected runtime still includes Open in Zed"
        );
        assert_eq!(bundle[1], "window.a = true;");
        assert_eq!(bundle[2], "window.b = true;");
    }

    #[test]
    fn runtime_bundle_injects_helper_build_date() {
        let bundled = bundled_runtime();
        let expected = format!("const helperBuildDate = {};", helper_build_date_literal());
        assert!(
            !bundled.contains("__CODEX_HELPER_BUILD_DATE__"),
            "packaged runtime left the build date placeholder in place"
        );
        assert!(
            bundled.contains(&expected),
            "packaged runtime did not inject the compile-time helper build date"
        );
        let expected_version = format!("const helperVersion = {};", helper_version_literal());
        assert!(
            !bundled.contains("__CODEX_HELPER_VERSION__"),
            "packaged runtime left the version placeholder in place"
        );
        assert!(
            bundled.contains(&expected_version),
            "packaged runtime did not inject the compile-time helper version"
        );
    }

    #[test]
    fn chatgpt_runtime_does_not_include_helper_settings_window() {
        let bundled = bundled_runtime();
        assert!(
            !bundled.contains("function startHelperSettingsApp("),
            "ChatGPT injected runtime included the Helper Settings window host"
        );
        assert!(
            !bundled.contains("installNativeHelperSettingsGroup"),
            "ChatGPT injected runtime still installs Helper settings into Codex Settings"
        );
    }

    #[test]
    fn settings_runtime_hosts_the_helper_settings_window() {
        let bundled = build_settings_runtime();
        let expected = format!("const helperBuildDate = {};", helper_build_date_literal());
        assert!(
            !bundled.contains("__CODEX_HELPER_BUILD_DATE__"),
            "settings runtime left the build date placeholder in place"
        );
        assert!(
            bundled.contains(&expected),
            "settings runtime did not inject the compile-time helper build date"
        );
        let expected_version = format!("const helperVersion = {};", helper_version_literal());
        assert!(
            !bundled.contains("__CODEX_HELPER_VERSION__"),
            "settings runtime left the version placeholder in place"
        );
        assert!(
            bundled.contains(&expected_version),
            "settings runtime did not inject the compile-time helper version"
        );
        assert!(
            bundled.contains("function startHelperSettingsApp("),
            "settings runtime is missing the Helper Settings window host"
        );
        assert!(
            bundled.contains("id: \"providers\""),
            "settings runtime is missing the Providers page"
        );
    }
}
