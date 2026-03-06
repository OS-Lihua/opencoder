.PHONY: build test lint clean install fmt check

# Default target
all: lint test build

# Build release binary
build:
	cargo build --release

# Run all tests
test:
	cargo test --all-features

# Run lints
lint:
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --all -- --check

# Format code
fmt:
	cargo fmt --all

# Type check without building
check:
	cargo check --all-targets --all-features

# Clean build artifacts
clean:
	cargo clean
	rm -rf dist/

# Install locally
install:
	cargo install --path crates/cli

# Run the CLI in dev mode
dev:
	cargo run -p opencoder-cli

# Run the server
serve:
	cargo run -p opencoder-cli -- serve

# Run a single prompt
run:
	@echo "Usage: make run PROMPT='your prompt here'"
	@test -n "$(PROMPT)" && cargo run -p opencoder-cli -- run "$(PROMPT)" || true
