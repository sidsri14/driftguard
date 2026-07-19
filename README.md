# DriftGuard

DriftGuard catches missing environment variables and broken AI output contracts before merge.

See the [engineering case study](docs/portfolio-case-study.md) for architecture,
validation evidence, scope boundaries, and a resume-ready project summary.

## 30-second quickstart

```bash
cargo install --git https://github.com/sidsri14/driftguard --locked && driftguard init && driftguard doctor && driftguard check
```

After the package is published to crates.io, the install command becomes:

```bash
cargo install driftguard-cli --locked
```

For development from this repository:

```bash
cargo run -- init
cargo run -- check
```

## Commands

```bash
driftguard init
driftguard check
driftguard check --since origin/main
driftguard check --since origin/main --env-scope changed
driftguard check --since origin/main --markdown
driftguard check --json
driftguard doctor
driftguard doctor --json
driftguard install-hook
```

## Configuration

`driftguard init` creates:

```toml
env_files = [".env.example"]
ignore_dirs = [".git", "node_modules", "target", "dist", "build", ".next", ".venv", "__pycache__"]
source_globs = ["**/*.js", "**/*.jsx", "**/*.ts", "**/*.tsx", "**/*.mjs", "**/*.cjs", "**/*.py", "**/*.rs"]
ignore_env_keys = []
```

Add prompt contracts as needed:

```toml
[prompts.router]
files = ["src/prompts/router.md"]
schema = "schemas/router.schema.json"
golden = "tests/golden/router/*.json"
```

`driftguard init` starts with no prompt mappings. Add a contract block like the
router example when the project has a prompt output schema. Prompt contracts
are active only when their mapped prompt files exist.

Use `--env-scope changed` with `--since` when you only want environment checks
against changed source files. The default `--env-scope all` scans the configured
source globs across the repository.

## False-positive controls

Ignore a known non-deployment key across the project:

```toml
ignore_env_keys = ["LOCAL_DEV_ONLY"]
```

Suppress a single source line or the next line when generated or dynamic code
cannot be represented in an env manifest:

```ts
const local = process.env.LOCAL_DEV_ONLY; // driftguard-ignore

// driftguard-ignore-next-line
const generated = process.env.GENERATED_KEY;
```

Keep suppressions narrow and reviewable. Commented-out code is ignored
automatically for supported JS/TS, Rust, and Python files.

## Machine-readable reports

`driftguard check --json` writes a versioned JSON document to stdout and keeps
the same exit codes as terminal output:

- `0`: contracts passed
- `1`: drift was detected
- `2`: DriftGuard could not execute the check

The top-level `format_version` is currently `1`. CI integrations should check
that value before consuming `environment` or `prompts` failures.
Execution failures use `verdict: "error"` and include an `error` string, so
stdout remains valid JSON for exit code `2`.

## What v0.3 checks

- JS/TS: `process.env.KEY` and `process.env["KEY"]`
- JS/TS destructuring: `const { KEY } = process.env`
- Python: `os.getenv("KEY")` and `os.environ["KEY"]`
- Rust: `std::env::var("KEY")` and `env!("KEY")`
- JS/TS/Rust line and block comments are ignored during env scanning
- Python line comments are ignored during env scanning
- Missing keys in configured env manifests
- Prompt golden fixtures that violate configured JSON schemas
- Prompt template variables like `{{user_payload}}` missing from golden fixture inputs
- Changed prompt markdown files without mapped contracts when `--since` is used
- Project-wide and line-level env scan suppressions
- Configuration health through `driftguard doctor`

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
`cargo install driftguard-cli --locked`.

When using `driftguard check --since origin/main`, keep `fetch-depth: 0` on
`actions/checkout@v4`. DriftGuard needs the base branch ref available locally to
compute changed prompt files.

The repo also includes:

- `.github/workflows/driftguard.yml` for normal CI validation
- `.github/workflows/driftguard-pr-comment.yml` for posting/updating PR comments
- `.github/workflows/quality.yml` for Linux, macOS, and Windows tests
- `.github/workflows/release.yml` for tag-based release binaries

## Examples

See `examples/broken-ai-app` for a compact app that demonstrates:

- missing environment variable detection
- prompt template input coverage
- prompt output JSON Schema validation

Run the complete pass/fail demo from the repository root:

```powershell
.\scripts\demo.ps1
```

```bash
./scripts/demo.sh
```

Play the recorded 30-second terminal session with asciinema:

```bash
asciinema play docs/demo.cast
```

## Benchmarks

Run the scanner benchmark locally:

```bash
cargo bench --bench env_scan
```

The benchmark generates 500 TypeScript files and measures the same scanner used
by the CLI. Performance depends on the machine and filesystem, so DriftGuard
does not claim a fixed scan time.

## Scope

DriftGuard validates deterministic contracts. It does not predict LLM quality,
execute prompts against model providers, inspect secret values, or replace a
language-specific compiler or security scanner.
