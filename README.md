# Sporo

Bare-metal (`no_std`) Rust for Sporo firmware runs on: an ESP32 driving an ST7789 TFT.

## Prerequisites

The ESP32 is Xtensa, which upstream `rustc` does not target, so the toolchain
comes from Espressif's fork:

With Nix, the flake installs the pinned toolchain for you:

```bash
nix run .#install-esp-toolchain
nix develop               # checks the versions on entry, sources export-esp.sh
```

Without Nix:

```bash
cargo install espup espflash
espup install \
  --toolchain-version 1.95.0.0 \
  --crosstool-toolchain-version 15.2.0_20250920 \
  --name esp-1.95.0.0
source ~/export-esp.sh    # needed in every shell that builds this
```

`espup install` downloads the forked Rust toolchain (installed under the name
[rust-toolchain.toml](rust-toolchain.toml) selects) and the `xtensa-esp32-elf`
linker. It is a large download the first time. The versions are pinned — see
[Reproducible builds](#reproducible-builds) for why, and what to do when
bumping them.

Inside the Nix dev shell (`nix develop`) the versions are checked on entry, and
the shell refuses to start if they do not match.

[firmware/build.rs](firmware/build.rs) checks for the linker up front, so
forgetting to source `export-esp.sh` gives you a one-line error instead of a wall
of linker failures.

## Layout

Two workspaces, because Cargo applies the target and `build-std` from
`.cargo/config.toml` across a whole workspace:

| Path | Contents |
| ---- | -------- |
| [crates/core/](crates/core/) | Wordlist, mnemonic, and entry-screen state. No chip dependencies, so it builds and tests for the host. |
| [crates/ui/](crates/ui/) | The screens, drawn into any `DrawTarget`. Also chip-free, so `make test` checks that what they draw lands on the panel. |
| [firmware/](firmware/) | Everything tied to the ESP32: display, keypad, and the Xtensa build settings in [firmware/.cargo/config.toml](firmware/.cargo/config.toml). |

The split is what lets `make test` run with no board, no espup environment, and
no cross-compilation. Anything that decides what a phrase *means* belongs in
`crates/core` for that reason.

## Build and flash

```bash
make build              # compile
make flash              # compile, flash, and open the serial monitor
make monitor            # serial monitor only
make test               # host tests; needs neither a board nor espup
make flash DEVICE=/dev/ttyUSB0
```

Or drive cargo directly. The device build has to run from `firmware/`, since
Cargo finds `.cargo/config.toml` relative to the working directory rather than
to the manifest:

```bash
cd firmware
cargo build --release
cargo run --release   # flashes via the runner configured in .cargo/config.toml
```

## Reproducible builds

Two people building the same commit **inside the Nix dev shell** get a
byte-identical firmware image, on different machines and under different home
directories. Prove it:

```bash
make check-reproducible   # builds twice from clean, compares the flashed .bin
```

CI runs this on every push, in the same `nix develop` shell, as the `build-nix`
job — so the claim is checked rather than assumed.

The guarantee is scoped to `nix develop`. The toolchain still lives outside the
Nix store — espup puts it in `~/.rustup` and `~/.espressif`, so `flake.lock`
cannot pin it — which is why the shell asserts the versions instead. Building
outside the shell with a different compiler will silently produce a different
image.

### Bumping the toolchain

Four places must move together, or the shell will refuse to start and CI will
fail:

1. `espRustVersion` / `espGccVersion` in [flake.nix](flake.nix) — these feed both
   the shell's assertion and `nix run .#install-esp-toolchain`
2. `channel` in [rust-toolchain.toml](rust-toolchain.toml)
3. `version` / `name` and the pin assertion in the `build` job of
   [.github/workflows/ci.yml](.github/workflows/ci.yml) (the `build-nix` job needs
   no change — it reads the pins from the flake)
4. The plain `espup install` command above, for non-Nix users

Then re-run `espup install` with the new versions and `make check-reproducible`.

## Hardware

Pin assignments mirror the `lilygo_ttgo_tdisplay`. 
The `esp32devkit` environment uses a different display wiring and a different
controller (ILI9163).

| Signal          | GPIO           | Notes                                    |
| --------------- | -------------- | ---------------------------------------- |
| TFT SCLK        | 18             |                                          |
| TFT MOSI        | 19             |                                          |
| TFT CS          | 5              |                                          |
| TFT DC          | 16             |                                          |
| TFT RST         | 23             |                                          |
| TFT backlight   | 4              | active high                              |
| Button          | 0              | active low; also a boot strapping pin    |
| Keypad rows     | 21, 27, 26, 22 | pull-up inputs; a pressed key reads low  |
| Keypad columns  | 33, 32, 25     | driven low one at a time, else high-Z    |


|                   | col 33 | col 32 | col 25 |
| ----------------- | ------ | ------ | ------ |
| **row 21**        | `1`    | `2`    | `3`    |
| **row 27**        | `4`    | `5`    | `6`    |
| **row 26**        | `7`    | `8`    | `9`    |
| **row 22**        | `*`    | `0`    | `#`    |
