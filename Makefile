.PHONY: build run dev clean

build:
	cd crates/frontend && trunk build --release
	cargo build --release -p backend

run: build
	./target/release/web-top

dev:
	cd crates/frontend && trunk build
	cargo build -p backend
	./target/debug/web-top

clean:
	cargo clean
	rm -rf crates/frontend/dist crates/frontend/target
