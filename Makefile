.PHONY: run dev mockup build release bundle bundle-full bundle-dmg bundle-full-dmg install-macos fetch-lsp lsp-slim check test fmt clippy clean vendor-gpui cli cli-install help

# Default: run Halo with hot reload
run dev:
	./scripts/dev.sh

# Design-review surface: the window mockup (traffic lights + title band).
mockup:
	HALO_MOCKUP=window cargo run -p halo

build release:
	./scripts/build.sh

vendor-gpui:
	./scripts/ensure-gpui-vendor.sh

fetch-lsp:
	./scripts/fetch-lsp.sh

# Strip + prune the staged language servers in place (~254 MB → ~189 MB).
lsp-slim:
	./scripts/lsp-slim.sh vendor/lsp

# Slim ~60 MB .app (resolves language servers from PATH/toolchains).
bundle:
	./scripts/bundle-macos.sh

# Full standalone bundle including offline language servers (~250 MB).
bundle-full:
	./scripts/bundle-macos.sh --with-lsp

# Drag-and-drop macOS DMG installer
bundle-dmg:
	./scripts/bundle-macos.sh --dmg

# DMG including offline language servers
bundle-full-dmg:
	./scripts/bundle-macos.sh --with-lsp --dmg

install-macos:
	./scripts/bundle-macos.sh --install

# Headless CLI — no GPUI, shares the app's database.
cli:
	cargo build -p halo-cli

cli-install: cli
	install -d "$$HOME/.local/bin"
	install -m 755 target/debug/halo-cli "$$HOME/.local/bin/halo-cli"
	@echo "installed $$HOME/.local/bin/halo-cli"

check:
	./scripts/check.sh

test:
	./scripts/test.sh

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

clean:
	cargo clean
	rm -rf dist

help:
	@echo "Halo"
	@echo ""
	@echo "  make run             Run with hot reload (debug)"
	@echo "  make mockup          Boot the window mockup"
	@echo "  make build           Release build"
	@echo "  make fetch-lsp       Stage bundled language servers (vendor/lsp)"
	@echo "  make lsp-slim        Strip + prune vendor/lsp in place (254 → 189 MB)"
	@echo "  make bundle          macOS .app → dist/Halo.app (~60 MB, uses PATH/toolchains)"
	@echo "  make bundle-full     .app with offline language servers (~250 MB)"
	@echo "  make bundle-dmg      .app + .dmg in dist/"
	@echo "  make bundle-full-dmg .app with LSP + .dmg"
	@echo "  make install-macos   Bundle and copy to /Applications"
	@echo "  make cli             Build the headless halo-cli binary"
	@echo "  make cli-install     Build halo-cli and copy to ~/.local/bin"
	@echo "  make check           fmt + clippy + test"
	@echo "  make test            cargo test --workspace"
	@echo "  make fmt             cargo fmt --all"
	@echo "  make clippy          cargo clippy"
	@echo "  make clean           cargo clean + remove dist/"
	@echo "  make vendor-gpui     Recreate patched gpui-base / gpui-component vendors"
	@echo ""
	@echo "Or: ./scripts/dev.sh | ./scripts/bundle-macos.sh"
