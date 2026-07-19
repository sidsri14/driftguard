# DriftGuard

DriftGuard catches missing environment variables and broken AI output contracts before merge.

## 30-second quickstart

```bash
cargo install driftguard && driftguard init && driftguard check
```

For local development from this repository:

```bash
cargo run -- init
cargo run -- check
```

Install directly from GitHub before a crates.io release:

```bash
cargo install --git https://github.com/sidsri14/driftguard --locked
```

## Commands

```bash
driftguard init
driftguard check
driftguard check --since origin/main
driftguard check --since origin/main --env-scope changed
driftguard check --since origin/main --markdown
driftguard install-hook
```

## Configuration

`driftguard init` creates:

```toml
env_files = [".env.example"]
ignore_dirs = [".git", "node_modules", "target", "dist", "build", ".next", ".venv", "__pycache__"]
source_globs = ["**/*.js", "**/*.jsx", "**/*.ts", "**/*.tsx", "**/*.mjs", "**/*.cjs", "**/*.py", "**/*.rs"]

[prompts.router]
files = ["src/prompts/router.md"]
schema = "schemas/router.schema.json"
golden = "tests/golden/router/*.json"

[prompts.extractor]
files = ["src/prompts/extraction_v2.md"]
schema = "schemas/extraction.schema.json"
golden = "tests/golden/extractor/*.json"
```

Prompt contracts are active only when their mapped prompt files exist.

Use `--env-scope changed` with `--since` when you only want environment checks
against changed source files. The default `--env-scope all` scans the configured
source globs across the repository.

## What v0.1 checks

- JS/TS: `process.env.KEY` and `process.env["KEY"]`
- JS/TS destructuring: `const { KEY } = process.env`
- Python: `os.getenv("KEY")` and `os.environ["KEY"]`
- Rust: `std::env::var("KEY")` and `env!("KEY")`
- Missing keys in configured env manifests
- Prompt golden fixtures that violate configured JSON schemas
- Prompt template variables like `{{user_payload}}` missing from golden fixture inputs
- Changed prompt markdown files without mapped contracts when `--since` is used

## Golden fixtures

Simple golden fixtures can be the expected output JSON directly:

```json
{
  "destination": "support"
}
```

For templated prompts, use an `input` object plus an `output` object. DriftGuard
checks that every `{{variable}}` in the prompt has a matching `input` key, then
validates `output` against the configured JSON Schema:

```json
{
  "input": {
    "user_payload": "I need help with billing"
  },
  "output": {
    "destination": "support"
  }
}
```

## GitHub Actions

The included `.github/workflows/driftguard.yml` validates this repository by
installing the local crate with `cargo install --path .`. After DriftGuard is
published, consumer repositories can replace that install step with
`cargo install driftguard --locked`.

When using `driftguard check --since origin/main`, keep `fetch-depth: 0` on
`actions/checkout@v4`. DriftGuard needs the base branch ref available locally to
compute changed prompt files.

The repo also includes:

- `.github/workflows/driftguard.yml` for normal CI validation
- `.github/workflows/driftguard-pr-comment.yml` for posting/updating PR comments
- `.github/workflows/release.yml` for tag-based release binaries

## Examples

See `examples/broken-ai-app` for a compact app that demonstrates:

- missing environment variable detection
- prompt template input coverage
- prompt output JSON Schema validation
