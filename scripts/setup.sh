#!/usr/bin/env bash
# setup.sh — one-shot dev environment bootstrap
# Usage: ./scripts/setup.sh [--check]
#   --check  Verify installed tools and versions without installing anything
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log()  { echo -e "\033[1;34m[setup]\033[0m $*"; }
ok()   { echo -e "\033[1;32m[ok]\033[0m $*"; }
warn() { echo -e "\033[1;33m[warn]\033[0m $*"; }
die()  { echo -e "\033[1;31m[error]\033[0m $*" >&2; exit 1; }

# ── Argument parsing ──────────────────────────────────────────────────────────
CHECK_ONLY=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) CHECK_ONLY=true ;;
    *) warn "Unknown argument: $1" ;;
  esac
  shift
done

# ── Platform detection ────────────────────────────────────────────────────────
OS="$(uname -s)"

# ── Version pins ──────────────────────────────────────────────────────────────
# Read expected Rust channel from rust-toolchain.toml
RUST_CHANNEL="stable"
if [[ -f rust-toolchain.toml ]]; then
  RUST_CHANNEL=$(grep -E '^channel\s*=' rust-toolchain.toml | sed 's/.*=\s*"\(.*\)"/\1/')
fi

# Pinned Stellar CLI version — update when upgrading the project toolchain.
# Must match the version installed in .devcontainer/devcontainer.json.
STELLAR_CLI_VERSION="21.7.7"

# ── Check-only mode ───────────────────────────────────────────────────────────
check_tools() {
  local failed=0

  echo ""
  log "Checking required tools (Rust channel: $RUST_CHANNEL)"

  # Rust / Cargo
  if command -v cargo &>/dev/null; then
    ok "Rust:        $(rustc --version 2>&1)"
  else
    warn "Rust:        NOT INSTALLED"
    failed=1
  fi

  # Stellar CLI
  if command -v stellar &>/dev/null; then
    local ver
    ver=$(stellar --version 2>&1 | head -1)
    ok "stellar-cli: $ver"
  else
    warn "stellar-cli: NOT INSTALLED"
    failed=1
  fi

  # Docker
  if command -v docker &>/dev/null; then
    ok "Docker:      $(docker --version 2>&1)"
    if docker compose version &>/dev/null 2>&1; then
      ok "Compose:     $(docker compose version 2>&1)"
    else
      warn "Compose:     NOT AVAILABLE (docker compose plugin required)"
      failed=1
    fi
  else
    warn "Docker:      NOT INSTALLED"
    failed=1
  fi

  echo ""
  if [[ $failed -eq 0 ]]; then
    ok "All required tools are present."
  else
    warn "Some tools are missing — run ./scripts/setup.sh to install them."
  fi
  return $failed
}

if $CHECK_ONLY; then
  check_tools
  exit $?
fi

# ── 1. .env ───────────────────────────────────────────────────────────────────
if [[ ! -f .env ]]; then
  log "Creating .env from .env.example"
  cp .env.example .env
  ok ".env created — edit it to customise endpoints"
else
  warn ".env already exists, skipping"
fi

# ── 2. Node dependencies (optional for frontend development) ──────────────────
if command -v node &>/dev/null && [[ -f package.json ]]; then
  NODE_VER=$(node -e "process.stdout.write(process.versions.node.split('.')[0])")
  if [[ "$NODE_VER" -ge 20 ]]; then
    log "Installing Node dependencies"
    if [[ -f package-lock.json ]]; then
      npm ci
    else
      npm install
    fi
    ok "Node dependencies installed: $(node --version)"
  else
    warn "Node.js 20+ required for frontend development (found $NODE_VER)"
  fi
else
  ok "Skipping Node.js setup (not needed for Soroban contract development)"
fi

# ── 3. Rust ───────────────────────────────────────────────────────────────────
# Known-good SHA256 for rustup-init.sh — update this when rustup releases a new version.
# Obtain from: https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/rustup-init.sha256
RUSTUP_SHA256="${RUSTUP_SHA256:-}"

install_rust() {
  [[ -n "$RUSTUP_SHA256" ]] || die "Set RUSTUP_SHA256 before running setup (see comment above)"
  local tmp
  tmp="$(mktemp)"
  trap 'rm -f "$tmp"' RETURN
  log "Downloading rustup installer"
  curl -fsSL --proto '=https' --tlsv1.2 https://sh.rustup.rs -o "$tmp"
  log "Verifying SHA256 checksum"
  echo "${RUSTUP_SHA256}  ${tmp}" | sha256sum --check --status \
    || die "rustup-init.sh checksum mismatch — aborting"
  bash "$tmp" -y --no-modify-path
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
}

if ! command -v cargo &>/dev/null; then
  install_rust
  ok "Rust installed (channel: $RUST_CHANNEL): $(rustc --version)"
else
  ok "Rust already installed: $(rustc --version)"
fi

# ── 4. Stellar CLI ────────────────────────────────────────────────────────────
install_stellar_cli() {
  log "Installing stellar-cli v${STELLAR_CLI_VERSION}"
  cargo install --locked "stellar-cli@${STELLAR_CLI_VERSION}" --features opt
}

if ! command -v stellar &>/dev/null; then
  install_stellar_cli
else
  INSTALLED_VER=$(stellar --version 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)
  if [[ "$INSTALLED_VER" != "$STELLAR_CLI_VERSION" ]]; then
    warn "stellar-cli version mismatch (found $INSTALLED_VER, want $STELLAR_CLI_VERSION)"
    log "Reinstalling stellar-cli v${STELLAR_CLI_VERSION}"
    install_stellar_cli
  fi
fi
ok "stellar-cli: $(stellar --version 2>&1 | head -1)"

# ── 5. Docker ─────────────────────────────────────────────────────────────────
install_docker_linux() {
  log "Installing Docker via get.docker.com"
  curl -fsSL https://get.docker.com | sh
  sudo usermod -aG docker "$USER" 2>/dev/null || true
  ok "Docker installed — you may need to log out and back in for group membership"
}

install_docker_macos() {
  if command -v brew &>/dev/null; then
    log "Installing Docker Desktop via Homebrew"
    brew install --cask docker
    ok "Docker Desktop installed — open Docker.app to start the daemon"
  else
    warn "Homebrew not found — install Docker Desktop manually: https://docs.docker.com/desktop/mac/"
  fi
}

if ! command -v docker &>/dev/null; then
  case "$OS" in
    Linux)  install_docker_linux ;;
    Darwin) install_docker_macos ;;
    *) warn "Unsupported OS '$OS' — install Docker manually: https://docs.docker.com/get-docker/" ;;
  esac
else
  ok "Docker: $(docker --version 2>&1)"
fi

if docker compose version &>/dev/null 2>&1; then
  ok "Docker Compose: $(docker compose version 2>&1)"
else
  warn "Docker Compose plugin not found — some scripts require it"
fi

# ── 6. Git hooks ──────────────────────────────────────────────────────────────
if [[ -d .git ]]; then
  log "Installing pre-commit lint hook"
  cat > .git/hooks/pre-commit <<'HOOK'
#!/usr/bin/env bash
npm run lint --silent || { echo "Lint failed — commit aborted"; exit 1; }
HOOK
  chmod +x .git/hooks/pre-commit
  ok "pre-commit hook installed"
fi

echo ""
ok "Setup complete! Next steps:"
echo "  stellar --version                   — verify Soroban CLI installation"
echo "  cd contracts/escrow && cargo build  — build escrow contract"
echo "  cd contracts/token && cargo build   — build token contract"
echo "  ./scripts/local-net.sh start        — start local Stellar node"
echo "  ./scripts/deploy.sh testnet         — deploy contracts to testnet"
echo ""
echo "Run './scripts/setup.sh --check' at any time to verify installed tools."
