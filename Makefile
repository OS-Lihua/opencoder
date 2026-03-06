.PHONY: build test lint clean install fmt check release-patch release-minor release-major

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

# Version bump and release (auto: commit + tag + push → triggers CI release)
release-patch:
	./scripts/version-bump.sh patch
	@VERSION=$$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/'); \
	git add -A && git commit -m "chore: release v$$VERSION" && \
	git tag "v$$VERSION" && git push origin master --tags

release-minor:
	./scripts/version-bump.sh minor
	@VERSION=$$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/'); \
	git add -A && git commit -m "chore: release v$$VERSION" && \
	git tag "v$$VERSION" && git push origin master --tags

release-major:
	./scripts/version-bump.sh major
	@VERSION=$$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/'); \
	git add -A && git commit -m "chore: release v$$VERSION" && \
	git tag "v$$VERSION" && git push origin master --tags
