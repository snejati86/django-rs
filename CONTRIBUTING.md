# Contributing to django-rs

Thank you for your interest in contributing to django-rs! This document provides guidelines for contributing to the project.

## Getting Started

### Prerequisites

- Rust 1.75 or later
- Git

### Setup

1. Fork and clone the repository:

   ```bash
   git clone https://github.com/<your-username>/django-rs.git
   cd django-rs
   ```

2. Build the project:

   ```bash
   cargo build --workspace
   ```

3. Run the tests:

   ```bash
   cargo test --workspace
   ```

## Development Workflow

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p django-rs-db

# Run a specific test
cargo test -p django-rs-db test_queryset_filter
```

### Code Style

We use `rustfmt` for formatting and `clippy` with pedantic lints:

```bash
# Format code
cargo fmt --all

# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --workspace --all-targets -- -D warnings
```

The workspace is configured with strict clippy lints including `pedantic` and `nursery` levels. `unsafe_code` is forbidden.

### Building Documentation

```bash
cargo doc --workspace --no-deps --open
```

## Making Changes

1. Create a feature branch from `main`:

   ```bash
   git checkout -b feature/my-change
   ```

2. Make your changes with clear, focused commits.

3. Ensure all checks pass:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

4. Push your branch and open a pull request.

## Pull Request Guidelines

- Keep PRs focused on a single change.
- Include tests for new functionality.
- Update documentation if you change public APIs.
- Ensure CI passes before requesting review.
- Write a clear description of what the PR does and why.

## Reporting Issues

When filing an issue, please include:

- A clear description of the problem or feature request.
- Steps to reproduce (for bugs).
- Expected vs actual behavior (for bugs).
- Rust version (`rustc --version`).

## License

By contributing, you agree that your contributions will be licensed under both MIT and Apache 2.0, consistent with the project's dual license.
