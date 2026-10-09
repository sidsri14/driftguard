# DriftGuard: GitLab Hackathon Submission ("Life After Code")

**Prize Pool:** $45,000 USD | **Category:** DevOps / AI / DevSecOps & SDLC Productivity  
**Repository:** [github.com/sidsri14/driftguard](https://github.com/sidsri14/driftguard) | [gitlab.com/sidsri14/driftguard](https://gitlab.com)  
**Author:** Siddharth Srivastava (`@sidsri14`)

---

## 1. Project Overview & Elevator Pitch

**DriftGuard** is an ultra-fast, local-first CI/CD evaluation engine and merge-request gate built in Rust. It eliminates the two most destructive failure modes in modern AI-agent and cloud applications:
1. **Undocumented Environment Variable Drift:** Code references new `process.env`, `os.environ`, or `std::env` keys without declaring them in `.env.example` or deployment manifests, triggering runtime startup panics in production containers.
2. **AI Tool & Output Schema Drift:** Agentic workflows and LLM tool invocations produce malformed JSON structures that violate downstream API contracts.

DriftGuard enforces these contracts deterministically in under 300ms inside GitLab CI pipelines before merge—with **zero cloud compute dependencies and zero code or secret egress**.

---

## 2. Architecture & Pipeline Flow

```text
       GitLab Merge Request Event (GitLab Duo / Developer)
                             │
                             ▼
              ┌─────────────────────────────┐
              │   GitLab CI Runner (Rust)   │
              └──────────────┬──────────────┘
                             │
            ┌────────────────┴────────────────┐
            ▼                                 ▼
┌───────────────────────┐         ┌───────────────────────┐
│  Environment Scanner  │         │ JSON Schema Validator │
│ (AST/Regex Diff Scope)│         │ (Prompt Golden Tests) │
└───────────┬───────────┘         └───────────┬───────────┘
            │                                 │
            └────────────────┬────────────────┘
                             ▼
              ┌─────────────────────────────┐
              │ Deterministic Rule Evaluator│
              └──────────────┬──────────────┘
                             │
         ┌───────────────────┴───────────────────┐
         ▼                                       ▼
 ┌───────────────┐                       ┌───────────────┐
 │ Pass (Exit 0) │                       │ Fail (Exit 1) │
 └───────┬───────┘                       └───────┬───────┘
         │                                       │
         ▼                                       ▼
 Merge Request Approved            GitLab MR Artifact & Warning
 Pipeline Continues                Blocks Breaking Deployment
```

---

## 3. GitLab Duo & CI/CD Integration

DriftGuard seamlessly integrates into `.gitlab-ci.yml` to guard GitLab Duo-assisted development and automated agent workflows:

```yaml
driftguard:contract-gate:
  stage: test
  image: rust:latest
  before_script:
    - git fetch origin $CI_DEFAULT_BRANCH
    - cargo install --path .
  script:
    - |
      if [ -n "$CI_MERGE_REQUEST_DIFF_BASE_SHA" ]; then
        driftguard check --since $CI_MERGE_REQUEST_DIFF_BASE_SHA --env-scope changed --markdown > driftguard-report.md
      else
        driftguard check --markdown > driftguard-report.md
      fi
      cat driftguard-report.md
  artifacts:
    name: "driftguard-contract-report-$CI_COMMIT_SHORT_SHA"
    expose_as: "DriftGuard Schema & Contract Report"
    paths:
      - driftguard-report.md
    when: always
```

---

## 4. Key Technical Differentiators

* **⚡ Ultra-Low Latency:** Scans 500+ files and evaluates schema assertions in ~200-270ms.
* **🔒 Zero-Trust Privacy:** 100% local-first. No API keys, no prompts, no source code sent to external third parties.
* **🎯 Git-Diff Scoped:** Scans only files modified in the active branch (`--since origin/main`), preventing noisy legacy debt failures.
* **🧪 Production Test Coverage:** 26 automated tests (19 unit/library tests + 7 end-to-end CLI integration tests), strict Clippy with zero warnings allowed.
* **📊 Dual Output Formats:** Emits human-readable terminal UI, clean Markdown for GitLab MR comments, and versioned JSON for machine observability.

---

## 5. Demo Video Script (3-Minute Walkthrough)

* **[0:00 - 0:40] Problem Demonstration:** Show a GitLab Duo AI agent generating a new API handler requiring `REDIS_CLUSTER_URL` and returning a new JSON structure, but forgetting to update `.env.example` and breaking the frontend schema.
* **[0:40 - 1:30] DriftGuard in Action:** Run `driftguard doctor` to verify runner health. Run `driftguard check --since origin/main` in terminal. Instant failure with precise file line numbers and missing schema keys.
* **[1:30 - 2:20] GitLab CI Automation:** Show GitLab CI pipeline triggering on Merge Request, running `driftguard:contract-gate`, rendering the Markdown report artifact, and blocking the merge.
* **[2:20 - 3:00] Resolution & Merge:** Add `REDIS_CLUSTER_URL` to `.env.example` and update schema. Pipeline turns green in 4 seconds. Merge is safely completed.

---

## 6. Verification & Run Commands

```bash
# Clone & install DriftGuard
git clone https://github.com/sidsri14/driftguard.git
cd driftguard
cargo test --all
cargo run -- init
cargo run -- doctor
cargo run -- check --markdown
```
