use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use glob::Pattern;
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
pub const JS_DESTRUCTURING_REGEX: &str = r#"(?s)\{(?P<body>[^}]*)\}\s*=\s*process\.env"#;
pub const JS_DESTRUCTURED_KEY_REGEX: &str = r#"^([a-zA-Z_][a-zA-Z0-9_]*)\s*(?::|=|$)"#;
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

pub fn check_env(
    root: &Path,
    config: &Config,
    changed_files: Option<&[String]>,
) -> Vec<EnvFailure> {
    let declared = read_env_manifest_keys(root, &config.env_files);
    let mut failures_by_location = BTreeMap::new();

    for env_use in scan_env_uses(root, config, changed_files) {
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

pub fn scan_env_uses(
    root: &Path,
    config: &Config,
    changed_files: Option<&[String]>,
) -> Vec<EnvUse> {
    let patterns = env_patterns();
    let source_globs = compile_source_globs(config);
    let mut uses = Vec::new();
    let changed_set = changed_files.map(|files| {
        files
            .iter()
            .map(|file| file.replace('\\', "/"))
            .collect::<BTreeSet<_>>()
    });

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !is_ignored_dir(entry, config))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        let normalized = normalize_path(root, path);
        if changed_set
            .as_ref()
            .is_some_and(|files| !files.contains(&normalized))
        {
            continue;
        }

        if !is_supported_source(&normalized, &source_globs) {
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

        if is_javascript_like(path) {
            uses.extend(scan_js_destructured_env(path, &raw));
        }
    }

    uses
}

fn scan_js_destructured_env(path: &Path, raw: &str) -> Vec<EnvUse> {
    let destructuring_regex =
        Regex::new(JS_DESTRUCTURING_REGEX).expect("JS destructuring regex should compile");
    let key_regex =
        Regex::new(JS_DESTRUCTURED_KEY_REGEX).expect("JS destructured key regex should compile");
    let mut uses = Vec::new();

    for captures in destructuring_regex.captures_iter(raw) {
        let Some(body_match) = captures.name("body") else {
            continue;
        };

        for (segment_offset, segment) in comma_segments(body_match.as_str()) {
            let trimmed = segment.trim_start();
            if trimmed.starts_with("...") {
                continue;
            }

            let leading_whitespace = segment.len() - trimmed.len();
            let Some(key_capture) = key_regex.captures(trimmed) else {
                continue;
            };
            let Some(key_match) = key_capture.get(1) else {
                continue;
            };
            let raw_key_offset =
                body_match.start() + segment_offset + leading_whitespace + key_match.start();

            uses.push(EnvUse {
                file: path.to_path_buf(),
                line: line_number_at(raw, raw_key_offset),
                key: key_match.as_str().to_string(),
            });
        }
    }

    uses
}

fn comma_segments(raw: &str) -> Vec<(usize, &str)> {
    let mut segments = Vec::new();
    let mut start = 0;

    for (index, byte) in raw.bytes().enumerate() {
        if byte == b',' {
            segments.push((start, &raw[start..index]));
            start = index + 1;
        }
    }

    segments.push((start, &raw[start..]));
    segments
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

fn compile_source_globs(config: &Config) -> Vec<Pattern> {
    config
        .source_globs
        .iter()
        .filter_map(|pattern| Pattern::new(&pattern.replace('\\', "/")).ok())
        .collect()
}

fn is_supported_source(normalized_path: &str, source_globs: &[Pattern]) -> bool {
    source_globs
        .iter()
        .any(|pattern| pattern.matches(normalized_path))
}

fn is_javascript_like(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs")
    )
}

fn is_ignored_dir(entry: &DirEntry, config: &Config) -> bool {
    entry.file_type().is_dir()
        && entry.file_name().to_str().is_some_and(|name| {
            IGNORED_DIRS.contains(&name) || config.ignore_dirs.iter().any(|ignored| ignored == name)
        })
}

pub fn normalize_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn line_number_at(raw: &str, byte_offset: usize) -> usize {
    raw[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::tempdir;

    use super::*;
    use crate::config::Config;

    #[test]
    fn detects_direct_bracket_python_and_rust_env_uses() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "src/client.ts",
            &[
                "const a = process.",
                "env.DEEPSEEK_API_KEY;\nconst b = process.",
                "env[\"DATABASE_URL\"];\n",
            ]
            .concat(),
        );
        write_file(
            dir.path(),
            "src/app.py",
            &[
                "import os\ntoken = os.",
                "getenv(\"OPENAI_API_KEY\")\ndsn = os.",
                "environ[\"DATABASE_URL\"]\n",
            ]
            .concat(),
        );
        write_file(
            dir.path(),
            "src/main.rs",
            &[
                "let token = std::env::",
                "var(\"ANTHROPIC_API_KEY\");\nlet mode = ",
                "env!(\"APP_ENV\");\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("DEEPSEEK_API_KEY"));
        assert!(keys.contains("DATABASE_URL"));
        assert!(keys.contains("OPENAI_API_KEY"));
        assert!(keys.contains("ANTHROPIC_API_KEY"));
        assert!(keys.contains("APP_ENV"));
    }

    #[test]
    fn detects_js_process_env_destructuring() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "src/config.ts",
            &[
                "const { DATABASE_URL, DEEPSEEK_API_KEY: deepseekKey } = process.",
                "env;\nconst {\n  OPENAI_API_KEY,\n  NODE_ENV = \"development\",\n} = process.",
                "env;\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("DATABASE_URL"));
        assert!(keys.contains("DEEPSEEK_API_KEY"));
        assert!(keys.contains("OPENAI_API_KEY"));
        assert!(keys.contains("NODE_ENV"));
    }

    #[test]
    fn ignores_heavy_directories() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "node_modules/pkg/index.js",
            &["const ignored = process.", "env.SHOULD_NOT_BE_SCANNED;"].concat(),
        );
        write_file(
            dir.path(),
            "src/index.js",
            &["const used = process.", "env.SHOULD_BE_SCANNED;"].concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("SHOULD_BE_SCANNED"));
        assert!(!keys.contains("SHOULD_NOT_BE_SCANNED"));
    }

    #[test]
    fn reports_missing_keys_from_env_manifest() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
        write_file(
            dir.path(),
            "src/index.ts",
            &[
                "const { DATABASE_URL, DEEPSEEK_API_KEY } = process.",
                "env;",
            ]
            .concat(),
        );

        let failures = check_env(
            dir.path(),
            &Config {
                env_files: vec![".env.example".to_string()],
                ignore_dirs: crate::config::default_ignore_dirs(),
                source_globs: crate::config::default_source_globs(),
                prompts: Default::default(),
            },
            None,
        );

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].key, "DEEPSEEK_API_KEY");
    }

    #[test]
    fn can_scope_env_scan_to_changed_files() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
        write_file(
            dir.path(),
            "src/changed.ts",
            &["const used = process.", "env.DEEPSEEK_API_KEY;"].concat(),
        );
        write_file(
            dir.path(),
            "src/unchanged.ts",
            &["const ignored = process.", "env.OPENAI_API_KEY;"].concat(),
        );

        let changed = vec!["src/changed.ts".to_string()];
        let failures = check_env(dir.path(), &test_config(), Some(&changed));

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].key, "DEEPSEEK_API_KEY");
    }

    fn keys_from_scan(root: &Path) -> BTreeSet<String> {
        scan_env_uses(root, &test_config(), None)
            .into_iter()
            .map(|env_use| env_use.key)
            .collect()
    }

    fn test_config() -> Config {
        Config {
            env_files: vec![".env.example".to_string()],
            ignore_dirs: crate::config::default_ignore_dirs(),
            source_globs: crate::config::default_source_globs(),
            prompts: Default::default(),
        }
    }

    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}
