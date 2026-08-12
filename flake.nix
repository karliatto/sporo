{
  description = "Spore - bare-metal Rust (no_std) firmware for ESP32";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustup
            espup

            # Flashing and the serial monitor. `cargo run` invokes this via the
            # runner configured in .cargo/config.toml.
            espflash

            # `make build | flash | monitor`
            gnumake

            # Links build.rs for the host; the Xtensa link is done by
            # xtensa-esp32-elf-gcc, which espup provides.
            gcc

            git
          ];

          shellHook = ''
            # espup installs outside the Nix store (into ~/.rustup and
            # ~/.espressif) and writes ~/export-esp.sh to put the Xtensa linker
            # on PATH and set LIBCLANG_PATH. So this shell is not hermetic:
            # flake.lock pins espup, but not what espup downloads.
            #
            # Reproducible builds need a fixed compiler anyway -- `build-std`
            # recompiles `core` with it, and its commit hash is baked into the
            # firmware -- so we pin the versions here and refuse to start if the
            # installed ones differ. That assertion is what makes "I built it in
            # the Nix shell" a checkable claim rather than a hope.
            #
            # Bumping the toolchain means moving three things together: these
            # variables, rust-toolchain.toml, and the espup command below.
            ESP_RUST_VERSION=1.95.0.0
            ESP_GCC_VERSION=15.2.0_20250920

            spore_install_cmd() {
              echo "      espup install \\"
              echo "        --toolchain-version $ESP_RUST_VERSION \\"
              echo "        --crosstool-toolchain-version $ESP_GCC_VERSION \\"
              echo "        --name esp-$ESP_RUST_VERSION"
            }

            echo "Spore dev shell"

            if [ ! -f "$HOME/export-esp.sh" ]; then
              echo ""
              echo "  The Xtensa toolchain is not installed yet. Run once:"
              echo ""
              spore_install_cmd
              echo ""
              echo "  then re-enter this shell to pick up ~/export-esp.sh."
              echo "  It is a large download the first time."
              exit 1
            fi

            . "$HOME/export-esp.sh"

            # rustc reports the espup version as a trailing "(1.95.0.0)", and the
            # GCC banner carries the crosstool-NG tag.
            spore_rustc=$(rustc --version 2>/dev/null)
            spore_gcc=$(xtensa-esp32-elf-gcc --version 2>/dev/null | head -1)

            case "$spore_rustc" in
              *"($ESP_RUST_VERSION)"*) ;;
              *)
                echo ""
                echo "  Xtensa Rust mismatch - builds here would not be reproducible."
                echo "    expected: $ESP_RUST_VERSION"
                echo "    found:    ''${spore_rustc:-none}"
                echo ""
                echo "  Install the pinned toolchain:"
                echo ""
                spore_install_cmd
                exit 1
                ;;
            esac

            case "$spore_gcc" in
              *"$ESP_GCC_VERSION"*) ;;
              *)
                echo ""
                echo "  Xtensa GCC mismatch - builds here would not be reproducible."
                echo "    expected: $ESP_GCC_VERSION"
                echo "    found:    ''${spore_gcc:-none}"
                echo ""
                echo "  Install the pinned toolchain:"
                echo ""
                spore_install_cmd
                exit 1
                ;;
            esac

            echo "  $spore_rustc"
            echo "  $spore_gcc"
            echo "  $(espflash --version 2>/dev/null)"
            echo "  make build | flash | monitor | check-reproducible"
          '';
        };
      }
    );
}
