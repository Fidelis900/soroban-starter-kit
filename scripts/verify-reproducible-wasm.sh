#!/usr/bin/env bash
# scripts/verify-reproducible-wasm.sh
#
# Verify that locally-built WASM binaries match the SHA-256 checksums
# published in the GitHub Release for a given git tag or commit.
#
# USAGE
# -----
#   # Compare local build against a published GitHub release tag:
#   ./scripts/verify-reproducible-wasm.sh --tag v1.2.3
#
#   # Compare local build against a pre-downloaded checksums file:
#   ./scripts/verify-reproducible-wasm.sh --checksums path/to/checksums.sha256
#
#   # Build + compare in CI (tag injected via RELEASE_TAG env var):
#   RELEASE_TAG=v1.2.3 ./scripts/verify-reproducible-wasm.sh
#
# EXIT CODES
# ----------
#   0  All hashes match
#   1  One or more hashes do not match (details printed to stdout)
#   2  Usage / environment error
#
# REQUIREMENTS
# ------------
#   - Docker (to run the reproducible build container)
#   - curl  (to fetch checksums from GitHub Releases)
#   - sha256sum (coreutils)
#   - jq (optional — used to pretty-print mismatch details)

set -euo pipefail

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
REPO="Fidelis900/soroban-starter-kit"
DOCKERFILE="docker/Dockerfile.reproducible"
IMAGE_TAG="soroban-reproducible-verify"
WASM_OUT_DIR="$(mktemp -d)"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Colour codes (disabled when not a terminal)
if [ -t 1 ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RESET='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; RESET=''
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
info()    { echo -e "${GREEN}[INFO]${RESET} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; }
die()     { error "$*"; exit 2; }
cleanup() { rm -rf "${WASM_OUT_DIR}"; }
trap cleanup EXIT

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
TAG=""
CHECKSUMS_FILE=""
SKIP_BUILD=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)        TAG="$2";           shift 2 ;;
    --checksums)  CHECKSUMS_FILE="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=true;    shift   ;;
    -h|--help)
      sed -n '2,/^# REQUIREMENTS/p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *) die "Unknown argument: $1. Use --help for usage." ;;
  esac
done

# Fall back to RELEASE_TAG env var if --tag not provided.
if [[ -z "$TAG" && -n "${RELEASE_TAG:-}" ]]; then
  TAG="$RELEASE_TAG"
fi

# ---------------------------------------------------------------------------
# Step 1: Obtain the reference checksums
# ---------------------------------------------------------------------------
if [[ -n "$CHECKSUMS_FILE" ]]; then
  info "Using provided checksums file: $CHECKSUMS_FILE"
  REFERENCE_CHECKSUMS="$CHECKSUMS_FILE"
elif [[ -n "$TAG" ]]; then
  REFERENCE_CHECKSUMS="${WASM_OUT_DIR}/reference-checksums.sha256"
  CHECKSUMS_URL="https://github.com/${REPO}/releases/download/${TAG}/checksums.sha256"
  info "Downloading checksums for release ${TAG} ..."
  if ! curl -fsSL "$CHECKSUMS_URL" -o "$REFERENCE_CHECKSUMS"; then
    die "Failed to download checksums from ${CHECKSUMS_URL}. Ensure the release exists and has a checksums.sha256 asset."
  fi
  info "Downloaded reference checksums from ${CHECKSUMS_URL}"
else
  die "Provide --tag <git-tag> or --checksums <file> (or set RELEASE_TAG env var)."
fi

# ---------------------------------------------------------------------------
# Step 2: Build the reproducible image (unless --skip-build is set)
# ---------------------------------------------------------------------------
if [[ "$SKIP_BUILD" == false ]]; then
  GIT_COMMIT_EPOCH=$(git -C "$REPO_ROOT" log -1 --format="%ct" 2>/dev/null || echo "0")
  info "Building reproducible Docker image (SOURCE_DATE_EPOCH=${GIT_COMMIT_EPOCH}) ..."
  docker build \
    --build-arg "GIT_COMMIT_EPOCH=${GIT_COMMIT_EPOCH}" \
    --no-cache \
    -f "${REPO_ROOT}/${DOCKERFILE}" \
    -t "${IMAGE_TAG}" \
    "${REPO_ROOT}"
  info "Docker build complete."
else
  info "Skipping Docker build (--skip-build)."
fi

# ---------------------------------------------------------------------------
# Step 3: Extract WASMs from the container
# ---------------------------------------------------------------------------
info "Extracting WASM artifacts from container ..."
docker run --rm \
  -v "${WASM_OUT_DIR}:/out" \
  "${IMAGE_TAG}" \
  sh -c "cp /wasm/*.wasm /out/ && cp /wasm/checksums.sha256 /out/local-checksums.sha256"
info "WASMs extracted to ${WASM_OUT_DIR}"

# ---------------------------------------------------------------------------
# Step 4: Compare checksums
# ---------------------------------------------------------------------------
info "Comparing checksums ..."
echo
echo "Reference checksums (from ${REFERENCE_CHECKSUMS}):"
cat "$REFERENCE_CHECKSUMS"
echo
echo "Local build checksums (from ${WASM_OUT_DIR}/local-checksums.sha256):"
cat "${WASM_OUT_DIR}/local-checksums.sha256"
echo

PASS=true
MISMATCH_COUNT=0
MATCH_COUNT=0

while IFS= read -r line; do
  # Format: "<hash>  <filename>"
  REF_HASH=$(echo "$line" | awk '{print $1}')
  FILENAME=$(echo "$line" | awk '{print $2}' | sed 's|^\./||')

  LOCAL_WASM="${WASM_OUT_DIR}/${FILENAME}"
  if [[ ! -f "$LOCAL_WASM" ]]; then
    warn "File not found in local build: ${FILENAME}"
    PASS=false
    ((MISMATCH_COUNT++)) || true
    continue
  fi

  LOCAL_HASH=$(sha256sum "$LOCAL_WASM" | awk '{print $1}')
  if [[ "$REF_HASH" == "$LOCAL_HASH" ]]; then
    echo -e "  ${GREEN}✓${RESET}  ${FILENAME}"
    ((MATCH_COUNT++)) || true
  else
    echo -e "  ${RED}✗${RESET}  ${FILENAME}"
    echo    "       Reference: ${REF_HASH}"
    echo    "       Local:     ${LOCAL_HASH}"
    PASS=false
    ((MISMATCH_COUNT++)) || true
  fi
done < "$REFERENCE_CHECKSUMS"

echo
if [[ "$PASS" == true ]]; then
  info "All ${MATCH_COUNT} WASM hashes match. Reproducible build verified. ✓"
  exit 0
else
  error "${MISMATCH_COUNT} hash mismatch(es) detected out of $((MATCH_COUNT + MISMATCH_COUNT)) total."
  error "The local build does not reproduce the published release artifacts."
  error ""
  error "Common causes:"
  error "  1. The git working tree has uncommitted changes."
  error "  2. The Rust toolchain or stellar-cli version in Dockerfile.reproducible"
  error "     does not match the toolchain used for the release."
  error "  3. A dependency was updated in Cargo.lock after the release was published."
  error "  4. SOURCE_DATE_EPOCH differs from the release commit timestamp."
  error ""
  error "To debug: run 'docker run --rm -it ${IMAGE_TAG} sh' and inspect /wasm/"
  exit 1
fi
