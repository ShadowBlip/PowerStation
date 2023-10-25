ALL_RS := $(shell find src -name '*.rs')

.PHONY: all
all: build debug

.PHONY: debug
debug: target/debug/lightning-bus 

target/debug/lightning-bus: $(ALL_RS) Cargo.lock
	cargo build

.PHONY: build
build: target/release/lightning-bus

target/release/lightning-bus: $(ALL_RS) Cargo.lock
	cargo build --release

.PHONY: run
run:
	cargo run

.PHONY: clean
clean:
	rm -rf target
