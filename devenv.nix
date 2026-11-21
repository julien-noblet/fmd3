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
    LLVM_COV = "${pkgs.rustc.llvmPackages.llvm}/bin/llvm-cov";
    LLVM_PROFDATA = "${pkgs.rustc.llvmPackages.llvm}/bin/llvm-profdata";
  };

  packages = with pkgs; [
    pkg-config
    openssl
    sqlite
    postgresql
    nodejs
    chromium
    kubernetes-helm
    cargo-llvm-cov
  ];

  enterShell = ''
    if [ -d /run/current-system ] && [ -z "''${PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH:-}" ]; then
      export PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH="${lib.getExe pkgs.chromium}"
    fi
  '';

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
    config = {
      settings.global.excludes = [
        "target/**"
        "e2e/node_modules/**"
        "e2e/playwright-report/**"
        "e2e/test-results/**"
        "e2e/package-lock.json"
        "lua/**"
        "locales/**"
        "docker-compose.yml"
        "charts/**"
        "lcov.info"
      ];
      programs = {
        rustfmt = {
          enable = true;
          edition = "2021";
        };
        nixfmt.enable = true;
        prettier = {
          enable = true;
          includes = [
            "*.ts"
            "*.tsx"
            "*.js"
            "*.json"
            "*.yml"
            "*.yaml"
          ];
        };
      };
    };
  };

  git-hooks.hooks = {
    rustfmt.enable = true;
    clippy.enable = true;
  };

  scripts = {
    check-helm.exec = ''
      set -euo pipefail
      helm lint charts/fmd3
      helm template fmd3 charts/fmd3 >/dev/null
      helm template fmd3 charts/fmd3 --set worker.enabled=true --set nats.enabled=true >/dev/null
      helm template fmd3 charts/fmd3 --set server.env.FMD_DEMO_MODE=true --set server.basePath=/reader --set ingress.enabled=true >/dev/null
    '';
    check.exec = ''
      set -euo pipefail
      treefmt --fail-on-change
      cargo clippy --workspace --all-targets -- -D warnings
      cargo test --workspace
      check-helm
    '';
    coverage.exec = ''
      set -euo pipefail
      cargo llvm-cov --workspace --lcov --output-path lcov.info --fail-under-lines 70
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
      cargo test -p fmd-core --test postgres_integration -- --ignored --nocapture
      cargo test -p fmd-cli --test nats_worker_integration -- --ignored --nocapture
    '';
    test-e2e.exec = ''
      set -euo pipefail
      cargo build --bin fmd
      npm --prefix e2e test
    '';
  };

  enterTest = ''
    export FMD_TEST_POSTGRES_URL="$FMD_DATABASE_URL"
    check
    test-integration
  '';
}
