# Repo API

Turn any code repository into a working API workspace.

Repo API discovers API behavior from source, compiles a canonical contract, generates OpenAPI, launches a stateful mock runtime, executes requests, and runs test suites from both CLI and desktop.

## What You Can Do Today

### Discover and compile your API
- Scan repositories for API evidence across multiple stacks.
- Compile a canonical contract and persist generated + effective artifacts.
- Export OpenAPI and request collections.

### Use the desktop app as an API workbench
- Open local repositories or GitHub repository URLs.
- Land on a Projects Dashboard with recent repositories and quick open/remove actions.
- Use guided onboarding with progress tracking and deferrable recommendations.
- Explore endpoint details, request/response structures, schema references, examples, security requirements, and source evidence.

### Run and manage a local mock API
- Start, restart, stop, and reset a stateful runtime.
- Export and import runtime state snapshots as JSON.
- Detect already-running runtime instances and keep UI state accurate.
- Safely handle externally managed runtime processes without crashing desktop dev sessions.

### Execute and save requests
- Execute requests with environment-based template resolution.
- Persist saved requests for reuse.
- Re-run saved requests and use them as test inputs.
- Track request history with redaction support.

### Run real tests
- Execute YAML suites from `.repo-api/tests/suites`.
- Resolve tests against saved requests and evaluate assertions on live responses.
- Export results as JUnit, JSON, or HTML.
- Auto-bootstrap a starter test suite during onboarding when no suites exist.

### Manage credentials and environment values
- Store credentials in Vault with redaction-aware handling.
- Import `.env` values into Vault.
- Preview `.env` import before writing.
- Import all variables (not only auth), with automatic secret typing when recognizable.

## Quick Start

### Full platform (mock backend + desktop)

Run from repository root:

```bash
make dev
```

Starts:
- Mock backend on `http://127.0.0.1:4010`
- Tauri desktop app (dev mode)

Stop both with `Ctrl+C`.

### CLI flow

```bash
cargo run -p api-cli -- scan ./fixtures/express-api
cargo run -p api-cli -- inspect ./fixtures/express-api
cargo run -p api-cli -- export openapi ./fixtures/express-api --output ./generated/openapi.yaml
cargo run -p api-cli -- mock ./fixtures/express-api --port 4010 --stateful
cargo run -p api-cli -- request --repository ./fixtures/express-api --environment mock --method POST --path /users --body '{"email":"user@example.com","name":"Alex"}'
```

## Desktop Workflow

1. Open a repository from Projects Dashboard.
2. Review API details in Explorer.
3. Send first request from Request Builder.
4. Start mock runtime and iterate with state import/export.
5. Run tests (starter suite auto-created if needed).
6. Store or import credentials/variables in Vault.

## Repository Layout (High Level)

- `apps/desktop`: React + Tauri desktop UI
- `apps/desktop/src-tauri`: Tauri host
- `crates/api-cli`: CLI entrypoint
- `crates/api-discovery`: evidence discovery engine
- `crates/api-compiler`: canonical contract compiler + exporters
- `crates/api-mock-runtime`: mock runtime server + state endpoints
- `crates/api-desktop-app`: desktop services + command layer
- `crates/api-testing`: test model/assertions/reports
- `crates/api-storage`: persistence for contracts, requests, tests, envs
- `crates/api-vault`: local secret store

## Current State Endpoints

Mock runtime internal endpoints:

- `GET /__api/health`
- `GET /__api/state/export`
- `POST /__api/state/import`
- `POST /__api/state/reset`
- `POST /__api/shutdown`

## Why Repo API Is Useful

- Bridges discovery and execution: not just docs, but runnable behavior.
- Works against uncertain or partial repositories and still produces value.
- Keeps onboarding practical by auto-creating missing first-run assets.
- Integrates mock runtime, requests, tests, and secrets in one loop.

## Roadmap: What Could Make This Wildly Better

### 1) Contract intelligence and quality gates
- Drift scoring between discovered and hand-edited contracts.
- Breaking-change risk grades with CI-ready policy gates.
- "Confidence heatmap" for endpoints and schemas with improvement hints.

### 2) Request Builder 2.0
- Endpoint-aware form mode (path/query/body fields generated from schema).
- Inline examples, payload templates, and one-click auth presets.
- Multi-environment compare mode (mock vs dev vs staging response diff).

### 3) Scenario-driven mock runtime
- Scenario timeline editor with conditions, branching, and seeded datasets.
- Persona packs (happy path, degraded dependency, auth failure, rate limit).
- Time-travel runtime state checkpoints with replay.

### 4) Testing that writes itself
- Auto-generate suites from endpoint and schema evidence with assertion suggestions.
- Golden-response snapshot tests with update review flow.
- Fuzz and boundary test generation from schema constraints.

### 5) Team workflows
- Shared workspace packs (saved requests, test suites, scenario bundles).
- Reviewable change plans for API evolution before implementation.
- Pull request assistant that annotates endpoint, schema, and test impact.

### 6) Observability and runtime diagnostics
- Live traffic traces against mock runtime with contract conformance overlays.
- Error clustering and remediation guidance for failing requests/tests.
- Latency budgets and contract-level SLO checks.

### 7) Secrets and environment ergonomics
- Bi-directional environment sync (Vault <-> runtime env profiles).
- Scoped variable sets (project, workspace, branch, user).
- Secret rotation reminders and stale credential detection.

### 8) Multi-repo and domain graph view
- Link services by shared schemas and inferred call relationships.
- Cross-repo endpoint dependency map for platform teams.
- Domain-centric explorer (business capability -> endpoints -> tests).

## Contributing

Contributions are welcome.

Good first contribution areas:
- Request Builder capability expansion
- Explorer schema visualization depth
- Test generation and failure diagnostics
- Vault and environment UX refinement

If you are adding features, keep the CLI and desktop flows aligned so first-run behavior remains consistent.
