# Repository-Aware API Explorer and Mock Runtime

`repo-api` scans a repository for API evidence, compiles a canonical contract, exports OpenAPI and request collections, and runs a mock server.

## Workflow

```bash
cargo run -p api-cli -- scan ./fixtures/express-api
cargo run -p api-cli -- inspect ./fixtures/express-api
cargo run -p api-cli -- export openapi ./fixtures/express-api --output ./generated/openapi.yaml
cargo run -p api-cli -- mock ./fixtures/express-api --port 4010 --stateful
cargo run -p api-cli -- request --repository ./fixtures/express-api --environment mock --method POST --path /users --body '{"email":"user@example.com","name":"Alex"}'
```

## Commands

- `repo-api init`
- `repo-api scan <repository>`
- `repo-api inspect <repository>`
- `repo-api export openapi <repository> --output <path>`
- `repo-api mock <repository> [--port 4010] [--seed 42] [--stateful]`
- `repo-api request --repository <path> [--endpoint <id> | --method <m> --path <p>]`
- `repo-api diff --before <contract> --after <contract>`
