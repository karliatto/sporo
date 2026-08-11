## Usage
#
#   $ make build       # compile the firmware
#   $ make flash       # compile, flash, then attach the serial monitor
#   $ make monitor     # attach the serial monitor only
#   $ make check       # type-check without producing a binary
#
# Requires the espup environment to be sourced first:
#
#   $ source ~/export-esp.sh
#

## Variables
BAUDRATE ?= 115200
DEVICE ?= /dev/ttyACM0
PROFILE ?= release

.PHONY: build\
check\
clean\
flash\
monitor

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
