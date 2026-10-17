#!/usr/bin/env bash
# Contract tests for scripts/build-source-archive.sh and
# scripts/trigger-obs-webhook.sh.
#
# Default (CI routine): fully offline. A disposable clone of this repository
# under mktemp carries a scratch vX.Y.Z tag; npm/cargo are PATH stubs, so
# there is no network and no full build. Nothing is created, tagged, or
# modified in the user's repository: the clone carries HEAD plus worktree
# copies of the two scripts under test, committed and tagged only in the
# clone, which the EXIT trap removes wholesale.
#
# Real-toolchain integration (network npm ci, cargo vendor, offline
# self-check build, prebuilt kpackage) is opt-in and runs separately:
#   OMNITILER_RELEASE_INTEGRATION=1 bash scripts/build-source-archive.test.sh
set -uo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
ARCHIVE_SCRIPT_SRC="$REPO_ROOT/scripts/build-source-archive.sh"
KPACKAGE_SCRIPT_SRC="$REPO_ROOT/scripts/build-kpackage.sh"
WEBHOOK_SCRIPT="$REPO_ROOT/scripts/trigger-obs-webhook.sh"
INTEGRATION="${OMNITILER_RELEASE_INTEGRATION:-0}"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/release-archive-test.XXXXXX")"
CLONE="$WORK/clone"
STUBS="$WORK/stubs"
OUT="$WORK/out"
FAIL=0
SERVER_PID=""

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 -- "$SERVER_PID" 2>/dev/null; then
    kill -- "$SERVER_PID" 2>/dev/null || true
  fi
  if [[ -n "${WORK:-}" && -d "${WORK:-}" && "${WORK:-}" != "/" && "${WORK:-}" != "$HOME" \
    && "${WORK:-}" != "$REPO_ROOT" ]]; then
    rm -rf -- "$WORK"
  else
    printf 'cleanup: refusing to remove work dir: %s\n' "${WORK:-<empty>}" >&2
  fi
}
trap cleanup EXIT

for tool in bash git tar gzip sha256sum grep awk node python3 curl jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'FAIL: missing required tool: %s\n' "$tool" >&2
    FAIL=1
  fi
done
if [[ "$INTEGRATION" == "1" ]]; then
  for tool in npm cargo; do
    if ! command -v "$tool" >/dev/null 2>&1; then
      printf 'FAIL: integration needs tool: %s\n' "$tool" >&2
      FAIL=1
    fi
  done
fi
if [[ "$FAIL" -ne 0 ]]; then
  exit 1
fi

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  FAIL=1
}

expect_failure() {
  local description="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    fail "expected failure but succeeded: $description"
  fi
}

# Disposable clone: scratch tags live and die here, never in the user repo.
git clone -q --no-tags "$REPO_ROOT" "$CLONE" >/dev/null 2>&1 || { fail "could not clone repo"; exit 1; }
# Negative fixture: the disposable tree must start notice-free even once the
# origin HEAD tracks the generated union notice, so the missing-notice run
# below always fails closed before any toolchain runs (no real npm/network).
git -C "$CLONE" rm -q --ignore-unmatch -- THIRD-PARTY-NOTICES >/dev/null 2>&1 \
  || { fail "could not drop union notice in clone"; exit 1; }
cp -- "$ARCHIVE_SCRIPT_SRC" "$KPACKAGE_SCRIPT_SRC" "$CLONE/scripts/" \
  || { fail "could not copy scripts under test into clone"; exit 1; }
git -C "$CLONE" -c user.email=release-test@local -c user.name=release-test \
  add scripts/build-source-archive.sh scripts/build-kpackage.sh >/dev/null 2>&1 \
  || { fail "could not stage scripts in clone"; exit 1; }
if ! git -C "$CLONE" diff --cached --quiet; then
  git -C "$CLONE" -c user.email=release-test@local -c user.name=release-test \
    commit -qm "scripts under test" >/dev/null 2>&1 \
    || { fail "could not commit scripts in clone"; exit 1; }
fi
VERSION="$(node -e "process.stdout.write(require('$CLONE/kwin/package.json').version)")" \
  || { fail "could not read clone kwin version"; exit 1; }
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { fail "clone version is not X.Y.Z: $VERSION"; exit 1; }
# Offline toolchain stubs for the default suite: emulate npm ci, the
# SOURCE_REV-baked bundle build, cargo vendor, and a successful offline
# cargo build. Real network builds run only under INTEGRATION=1.
# Installed before any archive run below, so even a regression that reaches
# the toolchain in a negative fixture still hits stubs, never real npm.
mkdir -p -- "$STUBS" "$OUT"
cat > "$STUBS/npm" <<'STUBEOF'
#!/usr/bin/env bash
set -euo pipefail
prefix=""
prev=""
for a in "$@"; do
  if [[ "$prev" == "--prefix" ]]; then prefix="$a"; fi
  prev="$a"
done
[[ -n "$prefix" && -d "$prefix" ]] || exit 1
if [[ " $* " == *" run build:installed "* ]]; then
  rev="${OMNITILER_SOURCE_REV:-}"
  [[ "$rev" =~ ^[0-9a-f]{40}([0-9a-f]{24})?$ ]] || exit 1
  mkdir -p "$prefix/contents/code"
  printf '// stub bundle for %s\n' "$rev" > "$prefix/contents/code/main.js"
  chmod 0644 "$prefix/contents/code/main.js"
  exit 0
fi
if [[ " $* " == *" ci "* ]]; then
  mkdir -p "$prefix/node_modules"
  exit 0
fi
exit 1
STUBEOF
cat > "$STUBS/cargo" <<'STUBEOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "vendor" ]]; then
  mkdir -p vendor
  printf '# stub vendor tree\n' > vendor/stub-crate-marker
  exit 0
fi
if [[ "${1:-}" == "build" ]]; then
  [[ " $* " == *" --offline "* && " $* " == *" --locked "* ]] || exit 1
  exit 0
fi
exit 1
STUBEOF
chmod +x -- "$STUBS/npm" "$STUBS/cargo"
if [[ "$INTEGRATION" != "1" ]]; then
  export PATH="$STUBS:$PATH"
fi

TAG="v$VERSION"
TOP_DIR="omnitiler-$VERSION"
ARCHIVE_SCRIPT="$CLONE/scripts/build-source-archive.sh"
KPACKAGE_SCRIPT="$CLONE/scripts/build-kpackage.sh"
git -C "$CLONE" tag "$TAG" >/dev/null 2>&1 || { fail "could not tag clone"; exit 1; }
# Fail closed: a tag without the committed union notice must not archive.
expect_failure "missing THIRD-PARTY-NOTICES" bash "$ARCHIVE_SCRIPT" --tag "$TAG" --output-dir "$OUT"
[[ ! -e "$OUT/$TOP_DIR.tar.gz" ]] || fail "failed run left an archive behind"
# The union notice travels as tracked source: stub it in the clone, commit,
# and move the scratch tag onto the notice-carrying tree.
printf 'stub union third-party notices for %s\n' "$TAG" > "$CLONE/THIRD-PARTY-NOTICES" \
  || { fail "could not stub union notice in clone"; exit 1; }
git -C "$CLONE" -c user.email=release-test@local -c user.name=release-test \
  add THIRD-PARTY-NOTICES >/dev/null 2>&1 \
  || { fail "could not stage union notice in clone"; exit 1; }
git -C "$CLONE" -c user.email=release-test@local -c user.name=release-test \
  commit -qm "stub union notice" >/dev/null 2>&1 \
  || { fail "could not commit union notice in clone"; exit 1; }
git -C "$CLONE" tag -f "$TAG" >/dev/null 2>&1 || { fail "could not retag clone"; exit 1; }
TAG_SHA="$(git -C "$CLONE" rev-parse "$TAG^{commit}")" || { fail "could not resolve clone tag"; exit 1; }

# Rejection contract: tag is required, stable-shaped, resolvable, and its
# source versions must match. None of these runs mutates the user repo.
expect_failure "missing --tag" bash "$ARCHIVE_SCRIPT" --output-dir "$OUT"
expect_failure "malformed tag" bash "$ARCHIVE_SCRIPT" --tag HEAD --output-dir "$OUT"
expect_failure "non-release tag shape" bash "$ARCHIVE_SCRIPT" --tag v0.1 --output-dir "$OUT"
expect_failure "unknown tag" bash "$ARCHIVE_SCRIPT" --tag v9.9.9 --output-dir "$OUT"
git -C "$CLONE" tag "v9.9.9" >/dev/null 2>&1 \
  || fail "could not tag clone for mismatch check"
expect_failure "version mismatch" bash "$ARCHIVE_SCRIPT" --tag v9.9.9 --output-dir "$OUT"
[[ ! -e "$OUT/omnitiler-9.9.9.tar.gz" ]] || fail "failed run left an archive behind"

# Success contract on the scratch clone tag.
bash "$ARCHIVE_SCRIPT" --tag "$TAG" --output-dir "$OUT" >/dev/null 2>&1 \
  || fail "archive build from $TAG failed"
ARCHIVE="$OUT/$TOP_DIR.tar.gz"
SIDECAR="$OUT/$TOP_DIR.tar.gz.sha256"
[[ -f "$ARCHIVE" && ! -L "$ARCHIVE" ]] || fail "archive missing: $ARCHIVE"
[[ -f "$SIDECAR" && ! -L "$SIDECAR" ]] || fail "sidecar missing: $SIDECAR"

if [[ -f "$SIDECAR" ]]; then
  SIDECAR_LINE="$(cat -- "$SIDECAR")"
  [[ "$SIDECAR_LINE" =~ ^[0-9a-f]{64}"  $TOP_DIR.tar.gz"$ ]] \
    || fail "sidecar has wrong shape: $SIDECAR_LINE"
  ACTUAL="$(sha256sum "$ARCHIVE" | awk '{print $1}')"
  [[ "${SIDECAR_LINE%%  *}" == "$ACTUAL" ]] || fail "sidecar digest does not match archive"
fi

ENTRIES="$WORK/entries"
tar -tzf "$ARCHIVE" > "$ENTRIES" 2>/dev/null || fail "archive is not a readable tar.gz"
if [[ -f "$ENTRIES" ]]; then
  while IFS= read -r entry; do
    [[ "$entry" == "$TOP_DIR/"* ]] || fail "entry outside top-level dir: $entry"
  done < "$ENTRIES"
  for member in "$TOP_DIR/kwin/contents/code/main.js" "$TOP_DIR/VERSION" \
    "$TOP_DIR/SOURCE_REV" "$TOP_DIR/kwin/package.json" "$TOP_DIR/Cargo.lock" \
    "$TOP_DIR/Cargo.toml" "$TOP_DIR/kwin/native-effect/CMakeLists.txt" \
    "$TOP_DIR/crates/tiler-kde-settings-exporter/Cargo.toml" \
    "$TOP_DIR/crates/tiler-kde-settings-exporter/src/main.rs" \
    "$TOP_DIR/kwin/native-effect/generated/kde_settings_descriptors.h" \
    "$TOP_DIR/kwin/native-effect/kdesettings_adapter.h" \
    "$TOP_DIR/kwin/native-effect/kdesettings_adapter.cpp" \
    "$TOP_DIR/kwin/native-effect/kdesettings_adapter_test.cpp" \
    "$TOP_DIR/THIRD-PARTY-NOTICES"; do
    grep -Fxq -- "$member" "$ENTRIES" || fail "archive lacks $member"
  done
  if grep -Fxq -- "$TOP_DIR/vendor/config.toml" "$ENTRIES"; then
    fail "archive must not carry a committed cargo config; vendor/ is referenced by path"
  fi
  grep -q -- "^$TOP_DIR/vendor/" "$ENTRIES" || fail "archive lacks vendored Rust crates"
  for banned in "$TOP_DIR/kwin/node_modules/" "$TOP_DIR/target/" "$TOP_DIR/dist/" "$TOP_DIR/kwin/dist/"; do
    grep -q -- "^$banned" "$ENTRIES" && fail "archive ships build-time dir: $banned"
  done
fi

EXTRACT="$WORK/extract"
mkdir -p -- "$EXTRACT"
tar -xzf "$ARCHIVE" -C "$EXTRACT" 2>/dev/null || fail "archive extraction failed"
if [[ -d "$EXTRACT/$TOP_DIR" ]]; then
  [[ "$(cat -- "$EXTRACT/$TOP_DIR/VERSION")" == "$VERSION" ]] || fail "VERSION file mismatch"
  [[ "$(cat -- "$EXTRACT/$TOP_DIR/SOURCE_REV")" == "$TAG_SHA" ]] || fail "SOURCE_REV file mismatch"
  [[ -f "$EXTRACT/$TOP_DIR/THIRD-PARTY-NOTICES" && ! -L "$EXTRACT/$TOP_DIR/THIRD-PARTY-NOTICES" ]] \
    || fail "extracted root union notice missing"
  cmp -s -- "$EXTRACT/$TOP_DIR/THIRD-PARTY-NOTICES" "$CLONE/THIRD-PARTY-NOTICES" \
    || fail "extracted root union notice differs from fixture file"
  grep -Fq -- "$TAG_SHA" "$EXTRACT/$TOP_DIR/kwin/contents/code/main.js" \
    || fail "extracted bundle lacks baked source rev"
fi

# Determinism only in the fast default suite (integration build is slow).
if [[ "$INTEGRATION" != "1" ]]; then
  OUT2="$WORK/out2"
  mkdir -p -- "$OUT2"
  bash "$ARCHIVE_SCRIPT" --tag "$TAG" --output-dir "$OUT2" >/dev/null 2>&1 \
    || fail "second archive build failed"
  DIGEST_A="$(sha256sum "$ARCHIVE" | awk '{print $1}')"
  DIGEST_B="$(sha256sum "$OUT2/$TOP_DIR.tar.gz" | awk '{print $1}')"
  [[ "$DIGEST_A" == "$DIGEST_B" ]] || fail "rebuild digest differs: $DIGEST_A vs $DIGEST_B"
fi

# Webhook trigger contract: inert without credentials, strict otherwise, and
# byte-exact delivery against a local capture server. No OBS contact.
SECRET="sekrit-ABCDEF-12345"
TOKEN="12345:$SECRET"
expect_failure "webhook missing args" bash "$WEBHOOK_SCRIPT" --tag "$TAG"
expect_failure "webhook bad tag" env OBS_TOKEN="$TOKEN" \
  bash "$WEBHOOK_SCRIPT" --tag HEAD --sha "$TAG_SHA" --repo owner/repo
expect_failure "webhook bad sha" env OBS_TOKEN="$TOKEN" \
  bash "$WEBHOOK_SCRIPT" --tag "$TAG" --sha deadbeef --repo owner/repo
expect_failure "webhook bad repo" env OBS_TOKEN="$TOKEN" \
  bash "$WEBHOOK_SCRIPT" --tag "$TAG" --sha "$TAG_SHA" --repo "not a slug"
expect_failure "webhook token without colon" env OBS_TOKEN="nocolon" \
  bash "$WEBHOOK_SCRIPT" --tag "$TAG" --sha "$TAG_SHA" --repo owner/repo
expect_failure "webhook non-numeric token id" env OBS_TOKEN="abc:$SECRET" \
  bash "$WEBHOOK_SCRIPT" --tag "$TAG" --sha "$TAG_SHA" --repo owner/repo

# Inert paths must exit 0 without attempting any connection: point at a
# closed port so any delivery attempt would fail loudly.
INERT_OUT="$(env -u OBS_TOKEN bash "$WEBHOOK_SCRIPT" --tag "$TAG" --sha "$TAG_SHA" \
  --repo owner/repo --endpoint http://127.0.0.1:9/trigger/workflow 2>&1)" \
  || fail "inert run without OBS_TOKEN failed"
[[ "$INERT_OUT" == *"inert"* ]] || fail "inert run did not report inert status"
INERT_OUT="$(env OBS_TOKEN="" bash "$WEBHOOK_SCRIPT" --tag "$TAG" --sha "$TAG_SHA" \
  --repo owner/repo --endpoint http://127.0.0.1:9/trigger/workflow 2>&1)" \
  || fail "inert run with empty OBS_TOKEN failed"

SERVER_PY="$WORK/capture-server.py"
CAPTURE="$WORK/capture.json"
PORT_FILE="$WORK/port"
cat > "$SERVER_PY" <<'PYEOF'
import http.server, json, sys
capture_path, port_path = sys.argv[1], sys.argv[2]
class Handler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(length)
        record = {
            "path": self.path,
            "event": self.headers.get("X-GitHub-Event"),
            "sig": self.headers.get("X-Hub-Signature-256"),
            "ctype": self.headers.get("Content-Type"),
            "body_hex": body.hex(),
        }
        with open(capture_path, "w", encoding="utf-8") as f:
            json.dump(record, f)
        self.send_response(200)
        self.end_headers()
    def log_message(self, *args):
        pass
server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
with open(port_path, "w", encoding="utf-8") as f:
    f.write(str(server.server_port))
server.serve_forever()
PYEOF
python3 "$SERVER_PY" "$CAPTURE" "$PORT_FILE" >/dev/null 2>&1 &
SERVER_PID=$!
PORT=""
for _ in $(seq 1 50); do
  [[ -s "$PORT_FILE" ]] && { PORT="$(cat -- "$PORT_FILE")"; break; }
  sleep 0.1
done
[[ "$PORT" =~ ^[0-9]+$ ]] || { fail "capture server did not start"; }
if [[ -n "$PORT" ]]; then
  DELIVER_OUT="$(env OBS_TOKEN="$TOKEN" bash "$WEBHOOK_SCRIPT" --tag "$TAG" \
    --sha "$TAG_SHA" --repo owner/repo \
    --endpoint "http://127.0.0.1:$PORT/trigger/workflow" 2>&1)" \
    || fail "webhook delivery to capture server failed: $DELIVER_OUT"
  [[ -f "$CAPTURE" ]] || fail "capture server recorded nothing"
fi
if [[ -f "$CAPTURE" ]]; then
  PATH_GOT="$(jq -r .path "$CAPTURE")"
  [[ "$PATH_GOT" == "/trigger/workflow?id=12345" ]] || fail "wrong request path: $PATH_GOT"
  [[ "$(jq -r .event "$CAPTURE")" == "push" ]] || fail "wrong event header"
  [[ "$(jq -r .ctype "$CAPTURE")" == "application/json" ]] || fail "wrong content type"
  BODY_HEX="$(jq -r .body_hex "$CAPTURE")"
  [[ "$BODY_HEX" =~ ^[0-9a-f]+$ ]] || fail "body hex malformed"
  BODY_JSON="$WORK/body.json"
  python3 -c "import binascii; open('$BODY_JSON','wb').write(binascii.unhexlify('$BODY_HEX'))"
  [[ "$(jq -r .ref "$BODY_JSON")" == "refs/tags/$TAG" ]] || fail "payload ref mismatch"
  [[ "$(jq -r .head_commit.id "$BODY_JSON")" == "$TAG_SHA" ]] || fail "payload sha mismatch"
  [[ "$(jq -r .repository.full_name "$BODY_JSON")" == "owner/repo" ]] || fail "payload repo mismatch"
  [[ "$(jq -r .created "$BODY_JSON")" == "true" ]] || fail "payload created flag mismatch"
  # Signature must be HMAC-SHA256 of the exact delivered bytes under the secret.
  EXPECTED_SIG="$(python3 -c "import binascii,hashlib,hmac; print('sha256=' + hmac.new(b'$SECRET', binascii.unhexlify('$BODY_HEX'), hashlib.sha256).hexdigest())")"
  [[ "$(jq -r .sig "$CAPTURE")" == "$EXPECTED_SIG" ]] || fail "webhook signature mismatch"
  # The token secret itself must never travel in path, headers, or body.
  if grep -Fq -- "$SECRET" "$CAPTURE"; then
    fail "token secret leaked into delivered request"
  fi
  # Existing-query endpoint shape (?id= appends with &).
  rm -f -- "$CAPTURE"
  env OBS_TOKEN="$TOKEN" bash "$WEBHOOK_SCRIPT" --tag "$TAG" --sha "$TAG_SHA" \
    --repo owner/repo --endpoint "http://127.0.0.1:$PORT/trigger/workflow?foo=bar" \
    >/dev/null 2>&1 || fail "delivery with query endpoint failed"
  [[ "$(jq -r .path "$CAPTURE")" == "/trigger/workflow?foo=bar&id=12345" ]] \
    || fail "query endpoint shape wrong: $(jq -r .path "$CAPTURE")"
fi

# Integration: real network toolchain on the scratch clone tag, plus the
# prebuilt kpackage path inside the clone (never the user worktree).
if [[ "$INTEGRATION" == "1" ]]; then
  INT_OUT="$WORK/int-out"
  mkdir -p -- "$INT_OUT"
  bash "$ARCHIVE_SCRIPT" --tag "$TAG" --output-dir "$INT_OUT" >/dev/null 2>&1 \
    || fail "integration archive build from $TAG failed"
  [[ -f "$INT_OUT/$TOP_DIR.tar.gz" ]] || fail "integration archive missing"
  if command -v kpackagetool6 >/dev/null 2>&1; then
    npm ci --prefix "$CLONE/kwin" --no-audit --no-fund >/dev/null 2>&1 \
      || fail "integration npm ci failed"
    npm --prefix "$CLONE/kwin" run build >/dev/null 2>&1 \
      || fail "integration kwin bundle build failed"
    KPACKAGE_OUT="$WORK/kpackage"
    mkdir -p -- "$KPACKAGE_OUT"
    if OMNITILER_PREBUILT_BUNDLE=1 NPM_BIN=/bin/false \
      bash "$KPACKAGE_SCRIPT" --output-dir "$KPACKAGE_OUT" >/dev/null 2>&1; then
      [[ -f "$KPACKAGE_OUT/omnitiler-kwin.kwinscript" ]] \
        || fail "integration prebuilt kpackage artifact missing"
    else
      fail "integration build-kpackage.sh failed with OMNITILER_PREBUILT_BUNDLE=1"
    fi
  else
    printf 'SKIP: integration prebuilt kpackage check (kpackagetool6 unavailable)\n'
  fi
fi

if [[ "$FAIL" -ne 0 ]]; then
  exit 1
fi
if [[ "$INTEGRATION" == "1" ]]; then
  printf 'build-source-archive integration tests passed\n'
else
  printf 'build-source-archive contract tests passed\n'
fi
