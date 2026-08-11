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
            # ~/.espressif) and writes this file to put the Xtensa linker on
            # PATH and set LIBCLANG_PATH. That means this shell is not fully
            # hermetic.
            if [ -f "$HOME/export-esp.sh" ]; then
              . "$HOME/export-esp.sh"

              echo "Spore dev shell"
              echo "  $(rustc --version 2>/dev/null || echo 'rustc unavailable - try: espup update')"
              echo "  $(espflash --version 2>/dev/null)"
              echo "  make build | flash | monitor"
            else
              echo "Spore dev shell"
              echo ""
              echo "  The Xtensa toolchain is not installed yet. Run once:"
              echo ""
              echo "      espup install"
              echo ""
              echo "  then re-enter this shell to pick up ~/export-esp.sh."
              echo "  It is a large download the first time."
            fi
          '';
        };
      }
    );
}
