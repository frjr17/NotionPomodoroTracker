dev:
    cargo run

test:
    cargo test

lint:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt

build:
    cargo build --release
