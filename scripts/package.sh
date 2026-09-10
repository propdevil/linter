#!/bin/sh
set -eu

# Release jobs build each native target before assembling the same plugin layout.
target=${1:?Usage: package.sh TARGET VERSION}
version=${2:?Usage: package.sh TARGET VERSION}
case "$target" in
    aarch64-apple-darwin|x86_64-apple-darwin|aarch64-unknown-linux-musl|x86_64-unknown-linux-musl) ;;
    *) printf 'Unsupported target: %s\n' "$target" >&2; exit 1 ;;
esac
printf '%s\n' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || exit 1
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
mkdir -p target/release-package dist
staging=$(mktemp -d "$root/target/release-package/staging.XXXXXX")
trap 'rm -rf "$staging"' EXIT
mkdir -p "$staging/linter/bin" "$staging/linter/.codex-plugin" "$staging/linter/.claude-plugin"
cp "target/$target/release/linter" "target/$target/release/linter-mcp" "$staging/linter/bin/"
cp -R skills configs "$staging/linter/"
cp README.md "$staging/linter/"
sed "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" \
    .codex-plugin/plugin.json > "$staging/linter/.codex-plugin/plugin.json"
sed "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" \
    .claude-plugin/plugin.json > "$staging/linter/.claude-plugin/plugin.json"
tar -czf "dist/linter-$target.tar.gz" -C "$staging" linter
printf 'Packaged dist/linter-%s.tar.gz\n' "$target"
