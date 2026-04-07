# List available recipes
default:
    @just --list

# Build all workspace crates
build:
    cargo build

# Run all Rust tests
test:
    cargo test --workspace

# Test a single crate (e.g., just test-crate plugin-core)
test-crate crate:
    cargo test -p {{crate}}

# Run clippy on all workspace crates
clippy:
    cargo clippy --workspace

# Run the backend server
server:
    cargo run -p server

# Run the worker
worker:
    cargo run -p worker

# Install all JS dependencies
install:
    pnpm install

# Generate API client from OpenAPI spec
gen-api url="http://127.0.0.1:3000/api-docs/openapi.json":
    node packages/web-sdk/scripts/generate-api.mjs {{url}}

# Build all JS packages (sdk before web)
build-js:
    pnpm build

# Build frontend SDK only
build-sdk:
    pnpm --filter @broccoli/web-sdk build

# Preview frontend
preview:
    pnpm --filter @broccoli/web preview

# Dev server for frontend
dev-web:
    pnpm --filter @broccoli/web dev

# All packages in parallel dev mode
dev:
    pnpm dev

# ESLint
lint-js:
    pnpm lint

# Prettier format
format:
    pnpm format

# Prettier format check
format-check:
    pnpm format:check

# Build all WASM plugins (debug)
build-plugins *args:
    cargo run -p broccoli-cli -- plugin build plugins/standard-checkers {{args}}
    cargo run -p broccoli-cli -- plugin build plugins/batch-evaluator {{args}}
    cargo run -p broccoli-cli -- plugin build plugins/ioi {{args}}
    cargo run -p broccoli-cli -- plugin build plugins/cooldown {{args}}
    cargo run -p broccoli-cli -- plugin build plugins/submission-limit {{args}}
    cargo run -p broccoli-cli -- plugin build plugins/icpc {{args}}

# Build all WASM plugins (release)
build-plugins-release:
    cargo run -p broccoli-cli -- plugin build plugins/standard-checkers --install --release
    cargo run -p broccoli-cli -- plugin build plugins/batch-evaluator --install --release
    cargo run -p broccoli-cli -- plugin build plugins/ioi --install --release
    cargo run -p broccoli-cli -- plugin build plugins/cooldown --install --release
    cargo run -p broccoli-cli -- plugin build plugins/submission-limit --install --release
    cargo run -p broccoli-cli -- plugin build plugins/icpc --install --release

# Build a single WASM plugin (e.g., just build-plugin plugins/standard-checkers)
build-plugin path *args:
    cargo run -p broccoli-cli -- plugin build {{path}} {{args}}

# Build a single WASM plugin in release mode
build-plugin-release path:
    cargo run -p broccoli-cli -- plugin build {{path}} --install --release

# Start PostgreSQL, Redis, and SeaweedFS
up:
    docker compose up -d

# Stop all services
down:
    docker compose down

# Dry-run publish server SDK to crates.io
publish-server-sdk-dry:
    cargo publish -p broccoli-server-sdk --dry-run

# Publish server SDK to crates.io
publish-server-sdk:
    cargo publish -p broccoli-server-sdk

# Dry-run publish CLI to crates.io
publish-cli-dry:
    cargo publish -p broccoli-cli --dry-run

# Publish CLI to crates.io
publish-cli:
    cargo publish -p broccoli-cli

# Run all checks (clippy + test + lint + format check)
check-all: clippy test lint-js format-check
