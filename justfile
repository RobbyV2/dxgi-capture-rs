default:
    @just --list

format:
    cargo fmt --all
    taplo fmt

format-check:
    cargo fmt --all -- --check
    taplo fmt --check

doc:
    cargo doc --workspace --all-features --no-deps --open --release

lint:
    cargo clippy --workspace --release --all-targets --all-features -- -D warnings
    
fix:
    cargo clippy --fix --allow-dirty --allow-staged --workspace --all-targets --all-features --release

build:
    cargo build --release --workspace --all-targets

build-lib:
    cargo build --release --package dxgi-capture-rs

build-example:
    cargo build --release --package example-stream

test:
    cargo test --workspace --release --all-targets --all-features
    cargo test --doc --release

example:
    cargo run --package example-stream --release

finalize:
    just format
    just doc
    just lint
    just test
    just build