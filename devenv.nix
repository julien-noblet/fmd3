{ config, pkgs, ... }:

{
  env = {
    RUST_BACKTRACE = 1;
    RUST_LOG = "info";
    FMD_NATS_URL = "nats://${config.services.nats.host}:${toString config.processes.nats.ports.main.value}";
    FMD_INTERNAL_TOKEN = "dev-internal-token"; # dev-only local token
  };

  packages = with pkgs; [
    pkg-config
    openssl
    sqlite
  ];

  languages.rust.enable = true;

  services.nats = {
    enable = true;
    jetstream.enable = true;
  };

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

  scripts = {
    check.exec = ''
      set -euo pipefail
      cargo fmt --check
      cargo clippy --workspace --all-targets -- -D warnings
      cargo test --workspace
    '';
    serve.exec = ''cargo run --bin fmd -- serve "$@"'';
    worker.exec = ''cargo run --bin fmd -- worker "$@"'';
    demo.exec = ''FMD_DEMO_MODE=true cargo run --bin fmd -- serve "$@"'';
  };

  enterTest = ''
    check
  '';
}
