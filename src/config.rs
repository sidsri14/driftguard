use std::{collections::BTreeMap, fs, io, path::Path};

use serde::{Deserialize, Serialize};

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
        let mut prompts = BTreeMap::new();
        prompts.insert(
            "router".to_string(),
            PromptContract {
                files: vec!["src/prompts/router.md".to_string()],
                schema: "schemas/router.schema.json".to_string(),
                golden: "tests/golden/router/*.json".to_string(),
            },
        );
        prompts.insert(
            "extractor".to_string(),
            PromptContract {
                files: vec!["src/prompts/extraction_v2.md".to_string()],
                schema: "schemas/extraction.schema.json".to_string(),
                golden: "tests/golden/extractor/*.json".to_string(),
            },
        );

        Self {
            env_files: default_env_files(),
            ignore_dirs: default_ignore_dirs(),
            source_globs: default_source_globs(),
            prompts,
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
