use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use glob::glob;
use jsonschema::JSONSchema;
use serde_json::Value;

use crate::{config::Config, env_scan::normalize_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptFailureKind {
    UnmappedChangedPrompt,
    MissingSchema,
    MissingGoldenFixtures,
    InvalidGoldenJson,
    SchemaViolation,
    InvalidSchema,
}

#[derive(Debug, Clone)]
pub struct PromptFailure {
    pub kind: PromptFailureKind,
    pub file: Option<PathBuf>,
    pub contract: Option<String>,
    pub schema: Option<PathBuf>,
    pub fixture: Option<PathBuf>,
    pub reason: String,
}

pub fn check_prompts(root: &Path, config: &Config, changed_files: &[String]) -> Vec<PromptFailure> {
    let mut failures = Vec::new();
    let contracts = resolve_contract_files(root, config);

    if !changed_files.is_empty() {
        let mapped_files: BTreeSet<String> = contracts
            .values()
            .flat_map(|files| files.iter().cloned())
            .collect();

        for changed in changed_files {
            if is_prompt_like_path(changed) && !mapped_files.contains(changed) {
                failures.push(PromptFailure {
                    kind: PromptFailureKind::UnmappedChangedPrompt,
                    file: Some(root.join(changed)),
                    contract: None,
                    schema: None,
                    fixture: None,
                    reason: "changed prompt file is not mapped in driftguard.toml".to_string(),
                });
            }
        }
    }

    for (name, contract) in &config.prompts {
        let changed_contract = changed_files.is_empty()
            || contracts
                .get(name)
                .is_some_and(|files| files.iter().any(|file| changed_files.contains(file)));

        if changed_contract {
            failures.extend(validate_contract(root, name, contract));
        }
    }

    failures
}

fn validate_contract(
    root: &Path,
    name: &str,
    contract: &crate::config::PromptContract,
) -> Vec<PromptFailure> {
    let mut failures = Vec::new();
    let prompt_files: Vec<PathBuf> = contract
        .files
        .iter()
        .flat_map(|pattern| glob_paths(root, pattern))
        .collect();
    if prompt_files.is_empty() {
        return failures;
    }

    let schema_path = root.join(&contract.schema);
    let schema_raw = match fs::read_to_string(&schema_path) {
        Ok(raw) => raw,
        Err(_) => {
            failures.push(PromptFailure {
                kind: PromptFailureKind::MissingSchema,
                file: None,
                contract: Some(name.to_string()),
                schema: Some(schema_path),
                fixture: None,
                reason: "schema file does not exist or cannot be read".to_string(),
            });
            return failures;
        }
    };

    let schema_json: Value = match serde_json::from_str(&schema_raw) {
        Ok(value) => value,
        Err(err) => {
            failures.push(PromptFailure {
                kind: PromptFailureKind::InvalidSchema,
                file: None,
                contract: Some(name.to_string()),
                schema: Some(schema_path),
                fixture: None,
                reason: format!("schema is not valid JSON: {err}"),
            });
            return failures;
        }
    };

    let compiled = match JSONSchema::compile(&schema_json) {
        Ok(compiled) => compiled,
        Err(err) => {
            failures.push(PromptFailure {
                kind: PromptFailureKind::InvalidSchema,
                file: None,
                contract: Some(name.to_string()),
                schema: Some(schema_path),
                fixture: None,
                reason: format!("schema cannot be compiled: {err}"),
            });
            return failures;
        }
    };

    let fixture_paths = glob_paths(root, &contract.golden);
    if fixture_paths.is_empty() {
        failures.push(PromptFailure {
            kind: PromptFailureKind::MissingGoldenFixtures,
            file: None,
            contract: Some(name.to_string()),
            schema: Some(schema_path),
            fixture: None,
            reason: "golden fixture glob matched no JSON files".to_string(),
        });
        return failures;
    }

    for fixture_path in fixture_paths {
        let raw = match fs::read_to_string(&fixture_path) {
            Ok(raw) => raw,
            Err(err) => {
                failures.push(PromptFailure {
                    kind: PromptFailureKind::InvalidGoldenJson,
                    file: None,
                    contract: Some(name.to_string()),
                    schema: Some(schema_path.clone()),
                    fixture: Some(fixture_path),
                    reason: format!("fixture cannot be read: {err}"),
                });
                continue;
            }
        };

        let fixture_json: Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(err) => {
                failures.push(PromptFailure {
                    kind: PromptFailureKind::InvalidGoldenJson,
                    file: None,
                    contract: Some(name.to_string()),
                    schema: Some(schema_path.clone()),
                    fixture: Some(fixture_path),
                    reason: format!("fixture is not valid JSON: {err}"),
                });
                continue;
            }
        };

        if let Err(errors) = compiled.validate(&fixture_json) {
            let reason = errors
                .map(|err| err.to_string())
                .next()
                .unwrap_or_else(|| "fixture failed schema validation".to_string());
            failures.push(PromptFailure {
                kind: PromptFailureKind::SchemaViolation,
                file: None,
                contract: Some(name.to_string()),
                schema: Some(schema_path.clone()),
                fixture: Some(fixture_path),
                reason,
            });
        };
    }

    failures
}

fn resolve_contract_files(root: &Path, config: &Config) -> BTreeMap<String, BTreeSet<String>> {
    config
        .prompts
        .iter()
        .map(|(name, contract)| {
            let mut files = BTreeSet::new();
            for pattern in &contract.files {
                let matches = glob_paths(root, pattern);
                if matches.is_empty() {
                    files.insert(pattern.replace('\\', "/"));
                } else {
                    for path in matches {
                        files.insert(normalize_path(root, &path));
                    }
                }
            }
            (name.clone(), files)
        })
        .collect()
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

fn is_prompt_like_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.ends_with(".md")
        && (normalized.starts_with("prompts/")
            || normalized.contains("/prompts/")
            || normalized.contains("prompt"))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::Path};

    use tempfile::tempdir;

    use super::*;
    use crate::config::{Config, PromptContract};

    #[test]
    fn validates_golden_fixture_against_schema() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "src/prompts/router.md", "Return a route.");
        write_file(
            dir.path(),
            "schemas/router.schema.json",
            r#"{
  "type": "object",
  "required": ["destination"],
  "properties": {
    "destination": { "type": "string" }
  },
  "additionalProperties": false
}"#,
        );
        write_file(
            dir.path(),
            "tests/golden/router/case_01.json",
            r#"{ "destination": "support" }"#,
        );

        let failures = check_prompts(dir.path(), &router_config(), &[]);

        assert!(failures.is_empty());
    }

    #[test]
    fn reports_schema_violating_golden_fixture() {
        let dir = tempdir().unwrap();
        write_file(dir.path(), "src/prompts/router.md", "Return a route.");
        write_file(
            dir.path(),
            "schemas/router.schema.json",
            r#"{
  "type": "object",
  "required": ["destination"],
  "properties": {
    "destination": { "type": "string" }
  },
  "additionalProperties": false
}"#,
        );
        write_file(
            dir.path(),
            "tests/golden/router/case_01.json",
            r#"{ "route": "support" }"#,
        );

        let failures = check_prompts(dir.path(), &router_config(), &[]);

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, PromptFailureKind::SchemaViolation);
    }

    #[test]
    fn reports_changed_unmapped_prompt_when_since_is_used() {
        let dir = tempdir().unwrap();
        let failures = check_prompts(
            dir.path(),
            &router_config(),
            &["src/prompts/new_router.md".to_string()],
        );

        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, PromptFailureKind::UnmappedChangedPrompt);
    }

    fn router_config() -> Config {
        let mut prompts = BTreeMap::new();
        prompts.insert(
            "router".to_string(),
            PromptContract {
                files: vec!["src/prompts/router.md".to_string()],
                schema: "schemas/router.schema.json".to_string(),
                golden: "tests/golden/router/*.json".to_string(),
            },
        );

        Config {
            env_files: vec![".env.example".to_string()],
            prompts,
        }
    }

    fn write_file(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}
