use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use glob::glob;
use glob::Pattern;
use regex::Regex;
use serde_json::Value;
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

pub const JS_ENV_REGEX: &str = r"process\.env(?:\?|\!)?\.([a-zA-Z_][a-zA-Z0-9_]*)";
pub const JS_BRACKET_REGEX: &str =
    r#"process\.env(?:\?|\!)?\s*\[\s*['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]\s*\]"#;
pub const JS_DESTRUCTURING_REGEX: &str = r#"(?s)\{(?P<body>[^}]*)\}\s*=\s*process\.env"#;
pub const JS_DESTRUCTURED_KEY_REGEX: &str = r#"^([a-zA-Z_][a-zA-Z0-9_]*)\s*(?::|=|$)"#;
pub const JS_IMPORT_META_ENV_REGEX: &str = r"import\.meta\.env\.([a-zA-Z_][a-zA-Z0-9_]*)";
pub const JS_DENO_ENV_REGEX: &str = r#"Deno\.env\.get\(\s*['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]"#;
pub const JS_BUN_ENV_REGEX: &str = r"Bun\.env\.([a-zA-Z_][a-zA-Z0-9_]*)";
pub const PY_GETENV_REGEX: &str = r#"os\.getenv\(\s*['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]"#;
pub const PY_ENVIRON_REGEX: &str = r#"os\.environ(?:\.get)?\(\s*['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]|os\.environ\s*\[\s*['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]\s*\]"#;
pub const RUST_ENV_VAR: &str = r#"std::env::var(?:_os)?\(\s*['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]"#; // driftguard-ignore
pub const RUST_ENV_MACRO: &str = r#"(?:env|option_env)!\(\s*['"]([a-zA-Z_][a-zA-Z0-9_]*)['"]"#;
pub const JS_COMPUTED_ENV_REGEX: &str = r#"process\.env(?:\?|\!)?\s*\[\s*[^'"\]\s][^\]]*\]"#; // driftguard-ignore
pub const PY_COMPUTED_ENVIRON_REGEX: &str =
    r#"os\.environ(?:\.get)?\(\s*[^'"\s][^)]*\)|os\.environ\s*\[\s*[^'"\]\s][^\]]*\]"#; // driftguard-ignore
pub const RUST_COMPUTED_ENV_REGEX: &str = r#"std::env::var(?:_os)?\(\s*[^'"\s][^)]*\)"#; // driftguard-ignore

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
    pub kind: EnvFailureKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvFailureKind {
    MissingDeclaration,
    DynamicKey,
}

pub fn check_env(
    root: &Path,
    config: &Config,
    changed_files: Option<&[String]>,
) -> Vec<EnvFailure> {
    let declared = read_declared_env_keys(root, config);
    let ignored = config
        .ignore_env_keys
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut failures_by_location = BTreeMap::new();

    for env_use in scan_env_uses(root, config, changed_files) {
        let is_dynamic_key = env_use.key == DYNAMIC_ENV_KEY;
        if is_dynamic_key || !declared.contains(&env_use.key) && !ignored.contains(&env_use.key) {
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
                    env_file: env_declaration_sources(config),
                    kind: if is_dynamic_key {
                        EnvFailureKind::DynamicKey
                    } else {
                        EnvFailureKind::MissingDeclaration
                    },
                },
            );
        }
    }

    failures_by_location.into_values().collect()
}

const DYNAMIC_ENV_KEY: &str = "<dynamic>";

fn read_declared_env_keys(root: &Path, config: &Config) -> BTreeSet<String> {
    let mut keys = read_env_manifest_keys(root, &config.env_files);
    for manifest in &config.deployment_manifests {
        for path in glob_paths(root, manifest) {
            keys.extend(read_deployment_manifest_keys(&path));
        }
    }
    keys
}

fn env_declaration_sources(config: &Config) -> String {
    config
        .env_files
        .iter()
        .chain(config.deployment_manifests.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
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

fn read_deployment_manifest_keys(path: &Path) -> BTreeSet<String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return BTreeSet::new();
    };

    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => read_json_env_keys(&raw),
        Some("toml") => read_toml_env_keys(&raw),
        Some("yaml" | "yml") => read_yaml_env_keys(&raw),
        _ => read_env_manifest_keys(
            path.parent().unwrap_or_else(|| Path::new(".")),
            &[path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string()],
        ),
    }
}

fn read_json_env_keys(raw: &str) -> BTreeSet<String> {
    let Ok(json) = serde_json::from_str::<Value>(raw) else {
        return BTreeSet::new();
    };
    let mut keys = BTreeSet::new();
    collect_json_env_keys(&json, &mut keys);
    keys
}

fn collect_json_env_keys(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "env" || key == "environment" {
                    if let Value::Object(env) = value {
                        keys.extend(env.keys().filter(|key| is_env_key(key)).cloned());
                    }
                }
                collect_json_env_keys(value, keys);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_json_env_keys(value, keys);
            }
        }
        _ => {}
    }
}

fn read_toml_env_keys(raw: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let mut in_environment = false;
    let assignment =
        Regex::new(r#"^([a-zA-Z_][a-zA-Z0-9_]*)\s*="#).expect("toml env regex should compile");

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_environment = matches!(
                trimmed,
                "[build.environment]"
                    | "[context.production.environment]"
                    | "[template.environment]"
            );
            continue;
        }
        if in_environment {
            if let Some(captures) = assignment.captures(trimmed) {
                keys.insert(captures[1].to_string());
            }
        }
    }

    keys
}

fn read_yaml_env_keys(raw: &str) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    let list_assignment = Regex::new(r#"^-\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*="#)
        .expect("yaml list env regex should compile");
    let map_assignment =
        Regex::new(r#"^([a-zA-Z_][a-zA-Z0-9_]*)\s*:"#).expect("yaml map env regex should compile");
    let name_assignment = Regex::new(r#"^-?\s*name:\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*$"#)
        .expect("yaml name env regex should compile");

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(captures) = list_assignment.captures(trimmed) {
            keys.insert(captures[1].to_string());
        } else if let Some(captures) = map_assignment.captures(trimmed) {
            keys.insert(captures[1].to_string());
        } else if let Some(captures) = name_assignment.captures(trimmed) {
            keys.insert(captures[1].to_string());
        }
    }

    keys
}

fn is_env_key(key: &str) -> bool {
    Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$")
        .expect("env key validation regex should compile")
        .is_match(key)
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
        let scan_source = strip_comments(path, &raw);
        let suppressed_lines = suppression_lines(&raw, &scan_source);

        for (line_index, line) in scan_source.lines().enumerate() {
            let line_number = line_index + 1;
            if suppressed_lines.contains(&line_number) {
                continue;
            }
            for pattern in &patterns {
                for captures in pattern.captures_iter(line) {
                    if let Some(key_match) = first_capture(&captures) {
                        uses.push(EnvUse {
                            file: path.to_path_buf(),
                            line: line_number,
                            key: key_match.as_str().to_string(),
                        });
                    }
                }
            }
            if has_dynamic_env_use(line, path) {
                uses.push(EnvUse {
                    file: path.to_path_buf(),
                    line: line_number,
                    key: DYNAMIC_ENV_KEY.to_string(),
                });
            }
        }

        if is_javascript_like(path) {
            uses.extend(
                scan_js_destructured_env(path, &scan_source)
                    .into_iter()
                    .filter(|env_use| !suppressed_lines.contains(&env_use.line)),
            );
        }
    }

    uses
}

fn glob_paths(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let absolute_pattern = root.join(pattern).to_string_lossy().replace('\\', "/");
    let Ok(paths) = glob(&absolute_pattern) else {
        return Vec::new();
    };

    paths
        .filter_map(Result::ok)
        .filter(|path| path.is_file())
        .collect()
}

fn has_dynamic_env_use(line: &str, path: &Path) -> bool {
    let patterns = if is_javascript_like(path) {
        [JS_COMPUTED_ENV_REGEX].as_slice()
    } else if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("py")
    ) {
        [PY_COMPUTED_ENVIRON_REGEX].as_slice()
    } else if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("rs")
    ) {
        [RUST_COMPUTED_ENV_REGEX].as_slice()
    } else {
        [].as_slice()
    };

    patterns.iter().any(|pattern| {
        Regex::new(pattern)
            .expect("dynamic env regex should compile")
            .is_match(line)
    })
}

fn suppression_lines(raw: &str, stripped: &str) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();

    for (index, (line, stripped_line)) in raw.lines().zip(stripped.lines()).enumerate() {
        let line_number = index + 1;
        let next_line_directive = line.contains("driftguard-ignore-next-line")
            && !stripped_line.contains("driftguard-ignore-next-line");
        let line_directive =
            line.contains("driftguard-ignore") && !stripped_line.contains("driftguard-ignore");

        if next_line_directive {
            lines.insert(line_number + 1);
        } else if line_directive {
            lines.insert(line_number);
        }
    }

    lines
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
        JS_IMPORT_META_ENV_REGEX,
        JS_DENO_ENV_REGEX,
        JS_BUN_ENV_REGEX,
        PY_GETENV_REGEX,
        PY_ENVIRON_REGEX,
        RUST_ENV_VAR,
        RUST_ENV_MACRO,
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).expect("env regex should compile"))
    .collect()
}

fn first_capture<'a>(captures: &'a regex::Captures<'a>) -> Option<regex::Match<'a>> {
    (1..captures.len()).find_map(|index| captures.get(index))
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

fn strip_comments(path: &Path, raw: &str) -> String {
    if is_javascript_like(path)
        || matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("rs")
        )
    {
        strip_slash_comments(raw)
    } else if matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("py")
    ) {
        strip_python_comments(raw)
    } else {
        raw.to_string()
    }
}

fn strip_slash_comments(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut index = 0;
    let mut state = CommentState::Normal;
    let mut escaped = false;

    while index < bytes.len() {
        let current = bytes[index] as char;
        let next = bytes.get(index + 1).map(|byte| *byte as char);

        match state {
            CommentState::Normal => match (current, next) {
                ('/', Some('/')) => {
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    state = CommentState::Line;
                }
                ('/', Some('*')) => {
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    state = CommentState::Block;
                }
                ('\'', _) => {
                    out.push(current);
                    index += 1;
                    escaped = false;
                    state = CommentState::SingleQuote;
                }
                ('"', _) => {
                    out.push(current);
                    index += 1;
                    escaped = false;
                    state = CommentState::DoubleQuote;
                }
                ('`', _) => {
                    out.push(current);
                    index += 1;
                    escaped = false;
                    state = CommentState::Backtick;
                }
                _ => {
                    out.push(current);
                    index += 1;
                }
            },
            CommentState::Line => {
                if current == '\n' {
                    out.push('\n');
                    state = CommentState::Normal;
                } else {
                    out.push(' ');
                }
                index += 1;
            }
            CommentState::Block => {
                if current == '*' && next == Some('/') {
                    out.push(' ');
                    out.push(' ');
                    index += 2;
                    state = CommentState::Normal;
                } else {
                    out.push(if current == '\n' { '\n' } else { ' ' });
                    index += 1;
                }
            }
            CommentState::SingleQuote => {
                out.push(current);
                index += 1;
                update_quote_state(current, &mut escaped, &mut state, CommentState::SingleQuote);
            }
            CommentState::DoubleQuote => {
                out.push(current);
                index += 1;
                update_quote_state(current, &mut escaped, &mut state, CommentState::DoubleQuote);
            }
            CommentState::Backtick => {
                out.push(current);
                index += 1;
                update_quote_state(current, &mut escaped, &mut state, CommentState::Backtick);
            }
        }
    }

    out
}

fn strip_python_comments(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut index = 0;
    let mut state = CommentState::Normal;
    let mut escaped = false;

    while index < bytes.len() {
        let current = bytes[index] as char;

        match state {
            CommentState::Normal => match current {
                '#' => {
                    out.push(' ');
                    index += 1;
                    state = CommentState::Line;
                }
                '\'' => {
                    out.push(current);
                    index += 1;
                    escaped = false;
                    state = CommentState::SingleQuote;
                }
                '"' => {
                    out.push(current);
                    index += 1;
                    escaped = false;
                    state = CommentState::DoubleQuote;
                }
                _ => {
                    out.push(current);
                    index += 1;
                }
            },
            CommentState::Line => {
                if current == '\n' {
                    out.push('\n');
                    state = CommentState::Normal;
                } else {
                    out.push(' ');
                }
                index += 1;
            }
            CommentState::SingleQuote => {
                out.push(current);
                index += 1;
                update_quote_state(current, &mut escaped, &mut state, CommentState::SingleQuote);
            }
            CommentState::DoubleQuote => {
                out.push(current);
                index += 1;
                update_quote_state(current, &mut escaped, &mut state, CommentState::DoubleQuote);
            }
            CommentState::Block | CommentState::Backtick => {
                out.push(current);
                index += 1;
            }
        }
    }

    out
}

fn update_quote_state(
    current: char,
    escaped: &mut bool,
    state: &mut CommentState,
    quote_state: CommentState,
) {
    if *escaped {
        *escaped = false;
    } else if current == '\\' {
        *escaped = true;
    } else if matches!(quote_state, CommentState::SingleQuote) && current == '\''
        || matches!(quote_state, CommentState::DoubleQuote) && current == '"'
        || matches!(quote_state, CommentState::Backtick) && current == '`'
    {
        *state = CommentState::Normal;
    }
}

#[derive(Clone, Copy)]
enum CommentState {
    Normal,
    Line,
    Block,
    SingleQuote,
    DoubleQuote,
    Backtick,
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
                "getenv(\"OPENAI_API_KEY\", \"fallback\")\ndsn = os.",
                "environ[\"DATABASE_URL\"]\n",
                "optional = os.environ.get(\"OPTIONAL_PY_KEY\")\n",
            ]
            .concat(),
        );
        write_file(
            dir.path(),
            "src/main.rs",
            &[
                "let token = std::env::",
                "var(\"ANTHROPIC_API_KEY\");\nlet optional = std::env::",
                "var_os(\"OPTIONAL_RUST_KEY\");\nlet mode = ",
                "env!(\"APP_ENV\");\nlet compile_optional = ",
                "option_env!(\"COMPILE_OPTIONAL_KEY\");\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("DEEPSEEK_API_KEY"));
        assert!(keys.contains("DATABASE_URL"));
        assert!(keys.contains("OPENAI_API_KEY"));
        assert!(keys.contains("OPTIONAL_PY_KEY"));
        assert!(keys.contains("ANTHROPIC_API_KEY"));
        assert!(keys.contains("OPTIONAL_RUST_KEY"));
        assert!(keys.contains("APP_ENV"));
        assert!(keys.contains("COMPILE_OPTIONAL_KEY"));
    }

    #[test]
    fn detects_common_js_runtime_env_uses() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "src/client.ts",
            &[
                "const optional = process.",
                "env?.OPTIONAL_CHAIN_KEY;\nconst asserted = process.",
                "env!.ASSERTED_KEY;\nconst spaced = process.",
                "env [ \"SPACED_BRACKET_KEY\" ];\nconst vite = import.meta.",
                "env.VITE_PUBLIC_KEY;\nconst deno = Deno.",
                "env.get(\"DENO_TOKEN\");\nconst bun = Bun.",
                "env.BUN_TOKEN;\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("OPTIONAL_CHAIN_KEY"));
        assert!(keys.contains("ASSERTED_KEY"));
        assert!(keys.contains("SPACED_BRACKET_KEY"));
        assert!(keys.contains("VITE_PUBLIC_KEY"));
        assert!(keys.contains("DENO_TOKEN"));
        assert!(keys.contains("BUN_TOKEN"));
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
                deployment_manifests: Vec::new(),
                ignore_dirs: crate::config::default_ignore_dirs(),
                source_globs: crate::config::default_source_globs(),
                ignore_env_keys: Vec::new(),
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

    #[test]
    fn ignores_env_patterns_inside_js_and_rust_comments() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "src/index.ts",
            &[
                "// const dead = process.",
                "env.OLD_LEGACY_TOKEN;\nconst active = process.",
                "env.ACTIVE_TOKEN;\n/*\nconst blockDead = process.",
                "env.BLOCKED_TOKEN;\n*/\n",
            ]
            .concat(),
        );
        write_file(
            dir.path(),
            "src/main.rs",
            &[
                "// let dead = std::env::",
                "var(\"OLD_RUST_TOKEN\");\nlet active = std::env::",
                "var(\"ACTIVE_RUST_TOKEN\");\n/* let block_dead = ",
                "env!(\"BLOCKED_RUST_TOKEN\"); */\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("ACTIVE_TOKEN"));
        assert!(keys.contains("ACTIVE_RUST_TOKEN"));
        assert!(!keys.contains("OLD_LEGACY_TOKEN"));
        assert!(!keys.contains("BLOCKED_TOKEN"));
        assert!(!keys.contains("OLD_RUST_TOKEN"));
        assert!(!keys.contains("BLOCKED_RUST_TOKEN"));
    }

    #[test]
    fn ignores_env_patterns_inside_python_comments() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "src/app.py",
            &[
                "# token = os.",
                "getenv(\"OLD_PY_TOKEN\")\ntoken = os.",
                "getenv(\"ACTIVE_PY_TOKEN\")\nprint(\"# not a comment\")\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("ACTIVE_PY_TOKEN"));
        assert!(!keys.contains("OLD_PY_TOKEN"));
    }

    #[test]
    fn honors_inline_suppression_directives() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "src/index.ts",
            &[
                "const local = process.",
                "env.LOCAL_ONLY; // driftguard-ignore\n// driftguard-ignore-next-line\nconst generated = process.",
                "env.GENERATED_KEY;\nconst active = process.",
                "env.ACTIVE_KEY;\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(!keys.contains("LOCAL_ONLY"));
        assert!(!keys.contains("GENERATED_KEY"));
        assert!(keys.contains("ACTIVE_KEY"));
    }

    #[test]
    fn honors_configured_ignored_env_keys() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
        write_file(
            dir.path(),
            "src/index.ts",
            &[
                "const local = process.",
                "env.LOCAL_ONLY;\nconst missing = process.",
                "env.REQUIRED_KEY;\n",
            ]
            .concat(),
        );
        let mut config = test_config();
        config.ignore_env_keys = vec!["LOCAL_ONLY".to_string()];

        let failures = check_env(dir.path(), &config, None);

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].key, "REQUIRED_KEY");
    }

    #[test]
    fn detects_dynamic_env_access_as_unverifiable() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
        write_file(
            dir.path(),
            "src/index.ts",
            "const token = process.env[`TENANT_${tenant}`];\n", // driftguard-ignore
        );
        write_file(
            dir.path(),
            "src/app.py",
            "import os\ntoken = os.environ.get(settings.token_key)\n",
        );
        write_file(
            dir.path(),
            "src/main.rs",
            "let token = std::env::var(config.token_key);\n", // driftguard-ignore
        );

        let failures = check_env(dir.path(), &test_config(), None);

        assert_eq!(failures.len(), 3);
        assert!(failures
            .iter()
            .all(|failure| failure.kind == EnvFailureKind::DynamicKey));
    }

    #[test]
    fn accepts_keys_declared_in_deployment_manifests() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), ".env.example", "DATABASE_URL=\n");
        write_file(
            dir.path(),
            "docker-compose.yml",
            "services:\n  api:\n    environment:\n      - DEEPSEEK_API_KEY=value\n      OPENAI_API_KEY: value\n",
        );
        write_file(
            dir.path(),
            "k8s/deployment.yaml",
            "env:\n  - name: ANTHROPIC_API_KEY\n    valueFrom:\n      secretKeyRef:\n        name: llm\n        key: token\n",
        );
        write_file(
            dir.path(),
            "vercel.json",
            r#"{ "env": { "VITE_PUBLIC_KEY": "value" } }"#,
        );
        write_file(
            dir.path(),
            "netlify.toml",
            "[build.environment]\nBUN_TOKEN = \"value\"\n",
        );
        write_file(
            dir.path(),
            "src/index.ts",
            &[
                "const deepseek = process.",
                "env.DEEPSEEK_API_KEY;\nconst openai = process.",
                "env.OPENAI_API_KEY;\nconst anthropic = process.",
                "env.ANTHROPIC_API_KEY;\nconst vite = import.meta.",
                "env.VITE_PUBLIC_KEY;\nconst bun = Bun.",
                "env.BUN_TOKEN;\n",
            ]
            .concat(),
        );
        let mut config = test_config();
        config.deployment_manifests = crate::config::default_deployment_manifests();

        let failures = check_env(dir.path(), &config, None);

        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn does_not_treat_string_contents_as_suppression_directives() {
        let dir = tempdir().unwrap();
        write_file(
            dir.path(),
            "src/index.ts",
            &[
                "const marker = \"driftguard-ignore\"; const active = process.",
                "env.ACTIVE_KEY;\n",
            ]
            .concat(),
        );

        let keys = keys_from_scan(dir.path());

        assert!(keys.contains("ACTIVE_KEY"));
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
            deployment_manifests: Vec::new(),
            ignore_dirs: crate::config::default_ignore_dirs(),
            source_globs: crate::config::default_source_globs(),
            ignore_env_keys: Vec::new(),
            prompts: Default::default(),
        }
    }

    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}
