use std::fs;

use crate::logging::DiagnosticLogger;
use crate::state_dir::StateDir;

const RUNTIME_HEADER: &str = "(() => {\n  if (typeof document === \"undefined\") return;\n\n";
const RUNTIME_FOOTER: &str = "\n})();\n";

include!(concat!(env!("OUT_DIR"), "/runtime_modules.rs"));

fn bundled_runtime() -> String {
    let body = RUNTIME_MODULE_SOURCES.join("\n\n");
    format!("{RUNTIME_HEADER}{body}{RUNTIME_FOOTER}")
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
            bundle[1].contains("zed_menu_item_injected")
                || bundle[1].contains("data-codex-helper-zed-menu-item")
        );
        assert_eq!(bundle[2], "window.a = true;");
        assert_eq!(bundle[3], "window.b = true;");
    }
}
