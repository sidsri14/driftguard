use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use regex::Regex;
use walkdir::{DirEntry, WalkDir};

use crate::config::Config;

pub const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "__pycache__",
];

pub const JS_ENV_REGEX: &str = r"process\.env\.([a-zA-Z_][a-zA-Z0-9_]*)";
pub const JS_BRACKET_REGEX: &str = r#"process\.env\[['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]\]"#;
pub const PY_GETENV_REGEX: &str = r#"os\.getenv\(['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]\)"#;
pub const PY_ENVIRON_REGEX: &str = r#"os\.environ\[['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]\]"#;
pub const RUST_ENV_VAR: &str = r#"std::env::var\(['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]\)"#;
pub const RUST_ENV_MACRO: &str = r#"env!\(['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]\)"#;

#[derive(Debug, Clone)]
pub struct EnvUse {
    pub file: PathBuf,
    pub line: usize,
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct EnvFailure {
    pub file: PathBuf,
    pub line: usize,
    pub key: String,
    pub env_file: String,
}

pub fn check_env(root: &Path, config: &Config) -> Vec<EnvFailure> {
    let declared = read_env_manifest_keys(root, &config.env_files);
    let mut failures_by_location = BTreeMap::new();

    for env_use in scan_env_uses(root) {
        if !declared.contains(&env_use.key) {
            failures_by_location.insert(
                (
                    normalize_path(root, &env_use.file),
                    env_use.line,
                    env_use.key.clone(),
                ),
                EnvFailure {
                    file: env_use.file,
                    line: env_use.line,
                    key: env_use.key,
                    env_file: config.env_files.join(", "),
                },
            );
        }
    }

    failures_by_location.into_values().collect()
}

fn read_env_manifest_keys(root: &Path, env_files: &[String]) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let key_regex = Regex::new(r"^(?:export\s+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*(?:=|$)")
        .expect("env key regex should compile");

    for env_file in env_files {
        let path = root.join(env_file);
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };

        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(captures) = key_regex.captures(trimmed) {
                keys.insert(captures[1].to_string());
            }
        }
    }

    keys
}

fn scan_env_uses(root: &Path) -> Vec<EnvUse> {
    let patterns = env_patterns();
    let mut uses = Vec::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !is_ignored_dir(entry))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if !is_supported_source(path) {
            continue;
        }

        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };

        for (line_index, line) in raw.lines().enumerate() {
            for pattern in &patterns {
                for captures in pattern.captures_iter(line) {
                    uses.push(EnvUse {
                        file: path.to_path_buf(),
                        line: line_index + 1,
                        key: captures[1].to_string(),
                    });
                }
            }
        }
    }

    uses
}

fn env_patterns() -> Vec<Regex> {
    [
        JS_BRACKET_REGEX,
        JS_ENV_REGEX,
        PY_GETENV_REGEX,
        PY_ENVIRON_REGEX,
        RUST_ENV_VAR,
        RUST_ENV_MACRO,
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).expect("env regex should compile"))
    .collect()
}

fn is_supported_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "py" | "rs")
    )
}

fn is_ignored_dir(entry: &DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| IGNORED_DIRS.contains(&name))
}

pub fn normalize_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
