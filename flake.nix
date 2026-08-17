{
  description = "Sporo - bare-metal Rust (no_std) firmware for ESP32";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # Bumping the toolchain means moving these two, rust-toolchain.toml, and
        # the pin the non-Nix CI job asserts. They feed both the installer below
        # and the shellHook's assertion, so those two can never disagree.
        espRustVersion = "1.95.0.0";
        espGccVersion = "15.2.0_20250920";

        # The one command that installs the pinned Xtensa toolchain, used by
        # humans on first checkout and by CI on a cache miss. It has to live
        # outside the dev shell: the shellHook below refuses to start until the
        # toolchain is present, so it cannot be the thing that installs it.
        install-esp-toolchain = pkgs.writeShellApplication {
          name = "install-esp-toolchain";
          # espup unpacks into ~/.rustup, which rustup expects to own.
          runtimeInputs = [ pkgs.espup pkgs.rustup ];
          text = ''
            espup install \
              --toolchain-version ${espRustVersion} \
              --crosstool-toolchain-version ${espGccVersion} \
              --name esp-${espRustVersion}
          '';
        };
      in
      {
        packages.install-esp-toolchain = install-esp-toolchain;

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustup
            espup

            # Flashing and the serial monitor. `cargo run` invokes this via the
            # runner configured in firmware/.cargo/config.toml.
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
            # firmware -- so the versions are pinned at the top of this flake and
            # this shell refuses to start if the installed ones differ. That
            # assertion is what makes "I built it in the Nix shell" a checkable
            # claim rather than a hope.
            #
            # Interpolated from the pins at the top of this flake, so the
            # assertions below check exactly what install-esp-toolchain installs.
            ESP_RUST_VERSION=${espRustVersion}
            ESP_GCC_VERSION=${espGccVersion}

            sporo_install_cmd() {
              echo "      nix run .#install-esp-toolchain"
            }

            echo "Sporo dev shell"

            if [ ! -f "$HOME/export-esp.sh" ]; then
              echo ""
              echo "  The Xtensa toolchain is not installed yet. Run once:"
              echo ""
              sporo_install_cmd
              echo ""
              echo "  then re-enter this shell to pick up ~/export-esp.sh."
              echo "  It is a large download the first time."
              exit 1
            fi

            . "$HOME/export-esp.sh"

            # rustc reports the espup version as a trailing "(1.95.0.0)", and the
            # GCC banner carries the crosstool-NG tag.
            sporo_rustc=$(rustc --version 2>/dev/null)
            sporo_gcc=$(xtensa-esp32-elf-gcc --version 2>/dev/null | head -1)

            case "$sporo_rustc" in
              *"($ESP_RUST_VERSION)"*) ;;
              *)
                echo ""
                echo "  Xtensa Rust mismatch - builds here would not be reproducible."
                echo "    expected: $ESP_RUST_VERSION"
                echo "    found:    ''${sporo_rustc:-none}"
                echo ""
                echo "  Install the pinned toolchain:"
                echo ""
                sporo_install_cmd
                exit 1
                ;;
            esac

            case "$sporo_gcc" in
              *"$ESP_GCC_VERSION"*) ;;
              *)
                echo ""
                echo "  Xtensa GCC mismatch - builds here would not be reproducible."
                echo "    expected: $ESP_GCC_VERSION"
                echo "    found:    ''${sporo_gcc:-none}"
                echo ""
                echo "  Install the pinned toolchain:"
                echo ""
                sporo_install_cmd
                exit 1
                ;;
            esac

            echo "  $sporo_rustc"
            echo "  $sporo_gcc"
            echo "  $(espflash --version 2>/dev/null)"
            echo "  make build | flash | monitor | check-reproducible"
          '';
        };
      }
    );
}
