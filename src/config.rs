use std::{collections::BTreeMap, fs, io, path::Path};

use serde::{Deserialize, Serialize};

use glob::Pattern;

pub const CONFIG_FILE: &str = "driftguard.toml";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_env_files")]
    pub env_files: Vec<String>,
    #[serde(default = "default_ignore_dirs")]
    pub ignore_dirs: Vec<String>,
    #[serde(default = "default_source_globs")]
    pub source_globs: Vec<String>,
    #[serde(default)]
    pub ignore_env_keys: Vec<String>,
    #[serde(default)]
    pub prompts: BTreeMap<String, PromptContract>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PromptContract {
    pub files: Vec<String>,
    pub schema: String,
    pub golden: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            env_files: default_env_files(),
            ignore_dirs: default_ignore_dirs(),
            source_globs: default_source_globs(),
            ignore_env_keys: Vec::new(),
            prompts: BTreeMap::new(),
        }
    }
}

pub fn default_env_files() -> Vec<String> {
    vec![".env.example".to_string()]
}

pub fn default_ignore_dirs() -> Vec<String> {
    [
        ".git",
        "node_modules",
        "target",
        "dist",
        "build",
        ".next",
        ".venv",
        "__pycache__",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

pub fn default_source_globs() -> Vec<String> {
    [
        "**/*.js", "**/*.jsx", "**/*.ts", "**/*.tsx", "**/*.mjs", "**/*.cjs", "**/*.py", "**/*.rs",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

pub fn init_config(root: &Path) -> io::Result<bool> {
    let path = root.join(CONFIG_FILE);
    if path.exists() {
        return Ok(false);
    }

    let rendered = toml::to_string_pretty(&Config::default())
        .expect("default DriftGuard config should serialize");
    fs::write(path, rendered)?;
    Ok(true)
}

pub fn load_config(root: &Path) -> Result<Config, String> {
    let path = root.join(CONFIG_FILE);
    let raw = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", CONFIG_FILE))?;
    toml::from_str(&raw).map_err(|err| format!("failed to parse {}: {err}", CONFIG_FILE))
}

pub fn validate_runtime(root: &Path, config: &Config) -> Result<(), String> {
    let mut errors = Vec::new();

    if config.env_files.is_empty() {
        errors.push("`env_files` must contain at least one manifest".to_string());
    }
    for env_file in &config.env_files {
        if !root.join(env_file).is_file() {
            errors.push(format!("configured env manifest `{env_file}` is missing"));
        }
    }

    if config.source_globs.is_empty() {
        errors.push("`source_globs` must contain at least one pattern".to_string());
    }
    for source_glob in &config.source_globs {
        if let Err(error) = Pattern::new(source_glob) {
            errors.push(format!("source glob `{source_glob}` is invalid: {error}"));
        }
    }

    for (name, contract) in &config.prompts {
        if contract.files.is_empty() {
            errors.push(format!("prompt contract `{name}` has no file patterns"));
        }
        for prompt_glob in &contract.files {
            if let Err(error) = Pattern::new(prompt_glob) {
                errors.push(format!(
                    "prompt contract `{name}` file glob `{prompt_glob}` is invalid: {error}"
                ));
            }
        }
        if let Err(error) = Pattern::new(&contract.golden) {
            errors.push(format!(
                "prompt contract `{name}` golden glob `{}` is invalid: {error}",
                contract.golden
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "configuration preflight failed:\n- {}\nRun `driftguard doctor` for details.",
            errors.join("\n- ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn runtime_validation_rejects_missing_env_manifest() {
        let dir = tempdir().unwrap();
        let error = validate_runtime(dir.path(), &Config::default()).unwrap_err();

        assert!(error.contains(".env.example"));
        assert!(error.contains("driftguard doctor"));
    }
}
