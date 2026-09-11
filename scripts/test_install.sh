#!/bin/sh
set -eu
repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
mkdir -p "$temporary/tools" "$temporary/assets" "$temporary/package/linter/bin" \
    "$temporary/package/linter/skills/software-design" "$temporary/package/linter/.codex-plugin" \
    "$temporary/package/linter/.claude-plugin"
payload="$temporary/package/linter"
cargo build --locked --manifest-path "$repo/Cargo.toml" -p linter-cli
cp "$repo/target/debug/linter" "$payload/bin/linter"
printf '#!/bin/sh\nexit 0\n' > "$payload/bin/linter-mcp"
chmod +x "$payload/bin/"*
printf 'Design skill\n' > "$payload/skills/software-design/SKILL.md"
printf '{"name":"linter","version":"0.1.0"}\n' > "$payload/.codex-plugin/plugin.json"
cp "$payload/.codex-plugin/plugin.json" "$payload/.claude-plugin/plugin.json"
for target in aarch64-apple-darwin x86_64-apple-darwin \
    aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
    tar -czf "$temporary/assets/linter-$target.tar.gz" -C "$temporary/package" linter
done
if command -v sha256sum >/dev/null 2>&1; then
    (cd "$temporary/assets" && sha256sum ./*.tar.gz) | sed 's|  ./|  |' > "$temporary/assets/SHA256SUMS"
else
    (cd "$temporary/assets" && shasum -a 256 ./*.tar.gz) | sed 's|  ./|  |' > "$temporary/assets/SHA256SUMS"
fi
cat > "$temporary/tools/curl" <<'SH'
#!/bin/sh
set -eu
url= output=
while [ "$#" -gt 0 ]; do
    case "$1" in
        https://*) url=$1 ;;
        -o) shift; output=$1 ;;
    esac
    shift
done
printf '%s\n' "$url" >> "$TEST_DOWNLOADS"
cp "$TEST_ASSETS/${url##*/}" "$output"
SH
cat > "$temporary/tools/codex" <<'SH'
#!/bin/sh
printf '%s %s\n' "${0##*/}" "$*" >> "$TEST_CALLS"
SH
cp "$temporary/tools/codex" "$temporary/tools/claude"
cat > "$temporary/tools/uname" <<'SH'
#!/bin/sh
case "$1" in -s) printf '%s\n' "$TEST_OS" ;; -m) printf '%s\n' "$TEST_ARCH" ;; esac
SH
chmod +x "$temporary/tools/"*
export TEST_ASSETS="$temporary/assets" TEST_CALLS="$temporary/calls" \
    TEST_DOWNLOADS="$temporary/downloads" TEST_OS=Darwin TEST_ARCH=arm64
export PATH="$temporary/tools:$PATH"
run() { sh "$repo/install.sh" "$@" > "$temporary/output" 2>&1; }
run both --root "$temporary/install space"
grep -F 'codex plugin add linter@propdevil-linter' "$TEST_CALLS" >/dev/null
grep -F 'claude plugin install linter@propdevil-linter --scope user' "$TEST_CALLS" >/dev/null
grep -F 'claude plugin update linter@propdevil-linter --scope user' "$TEST_CALLS" >/dev/null
cmp "$repo/skills/software-design/SKILL.md" \
    "$temporary/install space/plugins/linter/skills/software-design/SKILL.md"
grep -F "$temporary/install space/plugins/linter/bin/linter-mcp" \
    "$temporary/install space/plugins/linter/.mcp.json" >/dev/null
LINTER_VERSION=v0.1.0 run codex --root "$temporary/install space"
grep -F '/releases/download/v0.1.0/' "$TEST_DOWNLOADS" >/dev/null
printf 'PASS: bundle, both clients, paths with spaces, and repeat installation\n'

for pair in Darwin:x86_64 Linux:aarch64 Linux:x86_64; do
    TEST_OS=${pair%:*}; TEST_ARCH=${pair#*:}; export TEST_OS TEST_ARCH
    run claude --root "$temporary/$TEST_OS-$TEST_ARCH"
done
printf 'PASS: all four platform selections\n'

cp "$temporary/assets/SHA256SUMS" "$temporary/good-checksums"
printf '%064d  linter-x86_64-unknown-linux-musl.tar.gz\n' 0 > "$temporary/assets/SHA256SUMS"
cp "$TEST_CALLS" "$temporary/previous-calls"
if run codex --root "$temporary/install space"; then exit 1; fi
grep -F 'Checksum mismatch' "$temporary/output" >/dev/null
cmp "$TEST_CALLS" "$temporary/previous-calls"
test -x "$temporary/install space/plugins/linter/bin/linter-mcp"
cp "$temporary/good-checksums" "$temporary/assets/SHA256SUMS"
printf 'PASS: corrupt download preserves installation and never calls clients\n'

mkdir -p "$temporary/foreign/plugins/linter"
printf 'keep\n' > "$temporary/foreign/plugins/linter/sentinel"
if run codex --root "$temporary/foreign"; then exit 1; fi
grep -F 'unrelated directory' "$temporary/output" >/dev/null
test "$(cat "$temporary/foreign/plugins/linter/sentinel")" = keep
if run codex --repo invalid; then exit 1; fi
TEST_OS=Unsupported; export TEST_OS
if run codex --root "$temporary/unsupported"; then exit 1; fi
test ! -d "$temporary/unsupported"
printf 'PASS: unrelated directories, invalid repository, and unsupported platforms rejected\n'

"$temporary/install space/plugins/linter/bin/linter" install both \
    --root "$temporary/install space" > "$temporary/output" 2>&1
printf 'PASS: installed binary can reinstall itself\n'
cat > "$temporary/tools/claude" <<'SHCLIENT'
#!/bin/sh
exit 1
SHCLIENT
if "$temporary/install space/plugins/linter/bin/linter" install claude --root "$temporary/install space" \
    > "$temporary/output" 2>&1; then exit 1; fi
grep -F 'registration failed' "$temporary/output" >/dev/null
test -x "$temporary/install space/plugins/linter/bin/linter"
printf 'PASS: failed client registration retains the installed bundle\n'
