BIN := ./target/release/gatekeeper-rs

.PHONY: build run config dry test clean install

build:
	cargo build --release

run:
	$(BIN)

config:
	$(BIN) --config

dry:
	$(BIN) --dry-run

test:
	cargo test

clean:
	cargo clean

install:
	cargo install --path .
