# Caminho relativo: Makefile
#
# Atalhos de build/empacotamento para distribuição manual (ex.: testar num
# Windows que não tem toolchain Rust). Não substitui scripts/test.sh/.ps1
# (validação local equivalente ao CI) — propósito diferente.
#
# Cross-compile Linux -> Windows exige mingw-w64 + o target Rust instalado
# (ver docs/testing.md, seção "Cross-compile Linux -> Windows"); não
# tentado automaticamente aqui porque a pegadinha do posix/win32 exige
# uma escolha específica da máquina, registrada em .cargo/config.toml
# (local, não versionado).

WINDOWS_TARGET := x86_64-pc-windows-gnu
VERSION := $(shell grep -m1 '^version' Cargo.toml | cut -d '"' -f2)
DIST_DIR := dist
WINDOWS_ZIP := $(DIST_DIR)/agentry-windows-x86_64-$(VERSION).zip

.PHONY: help windows windows-build windows-clean

help:
	@echo "Alvos disponíveis:"
	@echo "  make windows        - cross-compila para Windows e gera o zip de distribuição em dist/"
	@echo "  make windows-build  - só cross-compila (target $(WINDOWS_TARGET)), sem empacotar"
	@echo "  make windows-clean  - remove o diretório dist/"

windows-build:
	cargo build --release --target $(WINDOWS_TARGET) -p agentry

windows: windows-build
	@mkdir -p $(DIST_DIR)
	@rm -f $(WINDOWS_ZIP)
	@zip -q -j $(WINDOWS_ZIP) target/$(WINDOWS_TARGET)/release/agentry.exe README.md LICENSE
	@echo "Pacote gerado: $(WINDOWS_ZIP)"

windows-clean:
	rm -rf $(DIST_DIR)

.DEFAULT_GOAL := help
