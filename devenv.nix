{
  config,
  pkgs,
  lib,
  ...
}:

{
  env = {
    RUST_BACKTRACE = 1;
    RUST_LOG = "info";
    FMD_DATABASE_URL = "postgres://postgres@127.0.0.1:5432/fmd";
    FMD_NATS_URL = "nats://${config.services.nats.host}:${toString config.processes.nats.ports.main.value}";
    FMD_SERVER_URL = "http://127.0.0.1:8089";
    FMD_INTERNAL_TOKEN = "dev-internal-token"; # dev-only local token
  };

  packages = with pkgs; [
    pkg-config
    openssl
    sqlite
    postgresql
    nodejs
  ];

  languages.rust.enable = true;

  services.postgres = {
    enable = true;
    listen_addresses = "127.0.0.1";
    initialDatabases = [ { name = "fmd"; } ];
    initialScript = ''
      DO $$
      BEGIN
        CREATE ROLE postgres WITH SUPERUSER LOGIN;
      EXCEPTION WHEN duplicate_object THEN
        NULL;
      END
      $$;
    '';
  };

  services.nats = {
    enable = true;
    jetstream.enable = true;
  };

  processes = lib.mkIf (!config.devenv.isTesting) {
    server.exec = "cargo run --bin fmd -- serve --bind 127.0.0.1:8089";
    worker.exec = "cargo run --bin fmd -- worker";
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
    serve.exec = ''
      [[ "$*" == *--bind* || "$*" == *-b* ]] || set -- --bind 127.0.0.1:8089 "$@"
      cargo run --bin fmd -- serve "$@"
    '';
    worker.exec = ''cargo run --bin fmd -- worker "$@"'';
    demo.exec = ''
      [[ "$*" == *--bind* || "$*" == *-b* ]] || set -- --bind 127.0.0.1:8089 "$@"
      FMD_DEMO_MODE=true cargo run --bin fmd -- serve "$@"
    '';
    test-integration.exec = ''
      set -euo pipefail
      export FMD_TEST_POSTGRES_URL="$FMD_DATABASE_URL"
      cargo test -p fmd-core test_postgres_backend_if_configured -- --nocapture
    '';
  };

  enterTest = ''
    export FMD_TEST_POSTGRES_URL="$FMD_DATABASE_URL"
    check
  '';
}
