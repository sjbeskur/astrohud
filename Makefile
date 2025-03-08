build:
	cargo build --release
#	@RUSTFLAGS='-C link-arg=-s' cargo build --target wasm32-unknown-unknown --release
#	cp target/wasm32-unknown-unknown/release/astrohud.wasm ./
serve:
#	@python3 -m http.server 8000
#	@python3 -m http.server 8000 --bind
	./target/release/astrohud-rest
