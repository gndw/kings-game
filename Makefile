SEED ?= 12648430
# Who to play as. The binary requires it; this is the only default there is.
PLAYER ?= char-tywin

# ponytail: WSLg puts its wayland socket outside XDG_RUNTIME_DIR, so winit falls
# back to a broken X11 and no window ever appears. Point it at the real one.
export XDG_RUNTIME_DIR := $(if $(wildcard /mnt/wslg/runtime-dir),/mnt/wslg/runtime-dir,$(XDG_RUNTIME_DIR))

.PHONY: run play test check fmt

run:            ## debug build, `make run SEED=1066 PLAYER=char-jon`
	cargo run --features bevy/dynamic_linking -- $(SEED) --player-character-id $(PLAYER)

play:           ## release build — the one you actually play
	cargo run --release -- $(SEED) --player-character-id $(PLAYER)

test:
	cargo test

check:          ## what CI would run
	cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test

fmt:
	cargo fmt
