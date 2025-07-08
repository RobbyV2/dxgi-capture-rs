default:
    @just --list

format:
    cargo fmt --all
    taplo fmt
    taplo fmt --check

lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    
lint-fix:
    cargo clippy --fix --allow-dirty --allow-staged --workspace --all-targets --all-features


build:
    cargo build --release --workspace --all-targets

test:
    cargo test --workspace --all-targets --all-features

finalize:
    just format
    just lint
    just test
    just build