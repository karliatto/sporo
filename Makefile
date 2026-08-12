## Usage
#
#   $ make build       # compile the firmware
#   $ make flash       # compile, flash, then attach the serial monitor
#   $ make monitor     # attach the serial monitor only
#   $ make check       # type-check without producing a binary
#   $ make check-reproducible   # prove the build is byte-for-byte reproducible
#
# Requires the espup environment to be sourced first:
#
#   $ source ~/export-esp.sh
#

## Variables
BAUDRATE ?= 115200
DEVICE ?= /dev/ttyACM0
PROFILE ?= release

ELF := target/xtensa-esp32-none-elf/$(PROFILE)/spore-rs

# Pins the build timestamp in the ESP-IDF app descriptor, which is part of the
# flashed image. Without this the descriptor records wall-clock time and no two
# builds ever match. Nix's stdenv already exports this exact value, so inside
# the dev shell it changes nothing; hard-assigning it means builds outside the
# shell land on the same timestamp too.
#
# esp-bootloader-esp-idf 0.5.0 reads the value as microseconds when it is really
# seconds, so the descriptor ends up reading 1970-01-01. Wrong, but fixed, which
# is all reproducibility asks of it.
export SOURCE_DATE_EPOCH := 315532800

.PHONY: build\
check\
clean\
flash\
monitor\
check-reproducible

build:
	cargo build --$(PROFILE)

check:
	cargo check --$(PROFILE)

# `cargo run` invokes espflash via the runner configured in .cargo/config.toml.
flash:
	sudo chown ${USER}:${USER} $(DEVICE)
	ESPFLASH_PORT=$(DEVICE) cargo run --$(PROFILE)

monitor:
	sudo chown ${USER}:${USER} $(DEVICE)
	espflash monitor --port $(DEVICE) --baud $(BAUDRATE)

clean:
	cargo clean

# Builds twice from scratch and compares the flashed image, not the ELF -- the
# .bin espflash produces is what actually reaches the device. Two clean builds,
# so expect a couple of minutes.
#
# This only proves determinism on this machine. The cross-machine guarantee also
# needs the pins flake.nix asserts; see the README.
check-reproducible:
	@set -e; \
	d=$$(mktemp -d); \
	trap 'rm -rf "$$d"' EXIT; \
	echo "==> build 1 of 2"; \
	cargo clean >/dev/null; \
	$(MAKE) --no-print-directory build; \
	espflash save-image --chip esp32 $(ELF) "$$d/a.bin" >/dev/null; \
	echo "==> build 2 of 2"; \
	cargo clean >/dev/null; \
	$(MAKE) --no-print-directory build; \
	espflash save-image --chip esp32 $(ELF) "$$d/b.bin" >/dev/null; \
	if cmp -s "$$d/a.bin" "$$d/b.bin"; then \
		echo "reproducible: $$(sha256sum <"$$d/a.bin" | cut -d' ' -f1)"; \
	else \
		echo "NOT reproducible - the two builds differ"; \
		exit 1; \
	fi
