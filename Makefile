# Caminho relativo: Makefile
#
# Atalhos de build/empacotamento para distribuição manual (ex.: testar num
# Windows que não tem toolchain Rust). Não substitui scripts/test.sh/.ps1
# (validação local equivalente ao CI) — propósito diferente.
#
# O build Linux é nativo (sem --target): reaproveita target/release, o mesmo
# diretório que `cargo build --release` já usa. Passar --target
# x86_64-unknown-linux-gnu explicitamente criaria uma segunda árvore de
# artefatos para o mesmo triplo — recompilação completa, sem ganho.
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
LINUX_TAR := $(DIST_DIR)/agentry-linux-x86_64-$(VERSION).tar.gz

.PHONY: help build linux linux-build \
        windows windows-build windows-clean dist-clean \
        disk clean-debug clean-incremental clean-all

help:
	@echo "Alvos disponíveis:"
	@echo "  make build              - compila o binário release para esta máquina (Linux nativo)"
	@echo "  make linux              - compila e gera o tar.gz de distribuição em dist/"
	@echo "  make linux-build        - só compila, sem empacotar (alias de build)"
	@echo ""
	@echo "  make windows            - cross-compila para Windows e gera o zip de distribuição em dist/"
	@echo "  make windows-build      - só cross-compila (target $(WINDOWS_TARGET)), sem empacotar"
	@echo ""
	@echo "  make dist-clean         - remove o diretório dist/"
	@echo "  make windows-clean      - idem (nome antigo, mantido)"
	@echo ""
	@echo "  make disk               - relatório de uso de disco de target/"
	@echo "  make clean-incremental  - remove só o cache incremental (build seguinte ainda aproveita deps)"
	@echo "  make clean-debug        - remove artefatos do perfil dev; preserva release e cross-compile"
	@echo "  make clean-all          - cargo clean completo (apaga target/ inteiro)"

build linux-build:
	cargo build --release -p agentry

linux: linux-build
	@mkdir -p $(DIST_DIR)
	@rm -f $(LINUX_TAR)
	@tar -czf $(LINUX_TAR) -C target/release agentry -C $(CURDIR) README.md LICENSE
	@echo "Pacote gerado: $(LINUX_TAR)"

windows-build:
	cargo build --release --target $(WINDOWS_TARGET) -p agentry

windows: windows-build
	@mkdir -p $(DIST_DIR)
	@rm -f $(WINDOWS_ZIP)
	@zip -q -j $(WINDOWS_ZIP) target/$(WINDOWS_TARGET)/release/agentry.exe README.md LICENSE
	@echo "Pacote gerado: $(WINDOWS_ZIP)"

dist-clean windows-clean:
	rm -rf $(DIST_DIR)

# --- Higiene de disco -------------------------------------------------------
#
# O perfil dev padrão gera debug info completo: cada binário de teste passa de
# 1 GB, e o Cargo não remove as versões de hash antigo. Sem limpeza periódica
# target/debug/deps cresce indefinidamente (chegou a 97 GB em julho/2026).

disk:
	@du -sh target 2>/dev/null || echo "target/ não existe"
	@du -sh target/* 2>/dev/null | sort -rh
	@echo "--- espaço livre ---"
	@df -h . | tail -1

clean-incremental:
	rm -rf target/debug/incremental target/release/incremental
	@echo "Cache incremental removido."

# Só o perfil dev: preserva target/release e os cross-targets (caros de refazer).
clean-debug:
	cargo clean --profile dev
	@echo "Artefatos de debug removidos; release e cross-compile preservados."

clean-all:
	cargo clean

.DEFAULT_GOAL := help
