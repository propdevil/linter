#!/bin/sh
set -eu

case "$(uname -s):$(uname -m)" in
    Darwin:arm64) target=aarch64-apple-darwin ;;
    Darwin:x86_64) target=x86_64-apple-darwin ;;
    Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
    Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-musl ;;
    *) echo 'Unsupported platform: use macOS or Linux on ARM64 or x86-64.' >&2; exit 1 ;;
esac

base=https://github.com/propdevil/linter/releases
case "${LINTER_VERSION:-latest}" in
    latest) base=$base/latest/download ;;
    *) base=$base/download/$LINTER_VERSION ;;
esac
asset=linter-$target.tar.gz
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
trap 'exit 1' HUP INT TERM

for file in "$asset" SHA256SUMS; do
    curl -fsSL --retry 3 --proto '=https' "$base/$file" -o "$temporary/$file"
done
expected=$(awk -v asset="$asset" '$2 == asset {print $1}' "$temporary/SHA256SUMS")
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$temporary/$asset" | awk '{print $1}')
else
    actual=$(shasum -a 256 "$temporary/$asset" | awk '{print $1}')
fi
if [ "$actual" != "$expected" ]; then
    echo 'Checksum mismatch; nothing was installed.' >&2
    exit 1
fi

tar -xzf "$temporary/$asset" -C "$temporary"
"$temporary/linter/bin/linter" install "$@" </dev/null
