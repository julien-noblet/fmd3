{ pkgs, ... }:

{
  env.RUST_BACKTRACE = 1;

  packages = with pkgs; [
    pkg-config
    openssl
    sqlite
  ];

  languages.rust.enable = true;

  services.nats.enable = true;

  treefmt = {
    enable = true;
    config.programs = {
      rustfmt = {
        enable = true;
        edition = "2021";
      };
      nixfmt.enable = true;
    };
  };

  git-hooks.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
  };

  scripts.check.exec = ''
    set -euo pipefail
    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
  '';

  enterTest = ''
    check
  '';
}
