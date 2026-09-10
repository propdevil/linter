#!/bin/sh
set -eu

fail() { printf 'Error: %s\n' "$*" >&2; exit 1; }
json_string() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'; }

main() {
    client=auto
    version=latest
    repo=propdevil/linter
    root=${LINTER_HOME:-"$HOME/.local/share/propdevil/linter"}
    while [ "$#" -gt 0 ]; do
        case "$1" in
            codex|claude|both|auto) client=$1 ;;
            --version|--root|--repo)
                [ "$#" -ge 2 ] || fail "$1 needs a value"
                option=$1; shift
                case "$option" in
                    --version) version=$1 ;;
                    --root) root=$1 ;;
                    --repo) repo=$1 ;;
                esac ;;
            -h|--help)
                printf '%s\n' 'Usage: install.sh [codex|claude|both] [--version vX.Y.Z] [--root PATH]'
                return ;;
            *) fail "Unknown argument: $1" ;;
        esac
        shift
    done
    case "$repo" in *[!a-zA-Z0-9_./-]*|''|/*|*/|*../*) fail 'Invalid GitHub repository' ;; esac
    [ "$(printf '%s' "$repo" | awk -F/ '{print NF}')" = 2 ] || fail 'Use owner/repository'
    case "$version" in *[!a-zA-Z0-9._+-]*|'') fail 'Invalid release version' ;; esac
    # Paths become JSON strings; reject control characters rather than corrupting client configuration.
    [ -n "$root" ] || fail 'Installation directory is empty'
    if printf '%s' "$root" | LC_ALL=C grep '[[:cntrl:]]' >/dev/null; then
        fail 'Installation directory cannot contain control characters'
    fi
    for tool in curl tar awk sed; do
        command -v "$tool" >/dev/null 2>&1 || fail "Required command not found: $tool"
    done
    clients=
    case "$client" in
        auto)
            for candidate in codex claude; do
                if command -v "$candidate" >/dev/null 2>&1; then clients="$clients $candidate"; fi
            done ;;
        both) clients='codex claude' ;;
        *) clients=$client ;;
    esac
    [ -n "$clients" ] || fail 'Install Codex CLI or Claude Code first, then rerun this command'
    for candidate in $clients; do
        command -v "$candidate" >/dev/null 2>&1 || fail "$candidate is not on PATH"
    done
    case "$(uname -s):$(uname -m)" in
        Darwin:arm64) target=aarch64-apple-darwin ;;
        Darwin:x86_64) target=x86_64-apple-darwin ;;
        Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
        Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-musl ;;
        *) fail 'Supported systems: macOS or Linux, Apple Silicon/ARM64 or x86-64' ;;
    esac
    if command -v sha256sum >/dev/null 2>&1; then
        hash_tool=sha256sum
    elif command -v shasum >/dev/null 2>&1; then
        hash_tool=shasum
    else
        fail 'sha256sum or shasum is required to verify the download'
    fi
    case "$version" in
        latest) base="https://github.com/$repo/releases/latest/download" ;;
        *) base="https://github.com/$repo/releases/download/$version" ;;
    esac
    temporary=$(mktemp -d)
    staging=
    trap 'rm -rf "$temporary"; if [ -n "$staging" ]; then rm -rf "$staging"; fi' EXIT
    trap 'exit 1' HUP INT TERM
    asset="linter-$target.tar.gz"
    printf 'Downloading %s (%s)…\n' "$repo" "$target"
    curl --fail --silent --show-error --location --retry 3 --proto '=https' \
        "$base/$asset" -o "$temporary/$asset"
    curl --fail --silent --show-error --location --retry 3 --proto '=https' \
        "$base/SHA256SUMS" -o "$temporary/SHA256SUMS"
    expected=$(awk -v asset="$asset" '$2 == asset {print $1}' "$temporary/SHA256SUMS")
    [ "${#expected}" = 64 ] || fail 'Missing or ambiguous archive checksum'
    case "$expected" in *[!a-fA-F0-9]*) fail 'Invalid archive checksum' ;; esac
    if [ "$hash_tool" = shasum ]; then
        actual=$(shasum -a 256 "$temporary/$asset" | awk '{print $1}')
    else
        actual=$(sha256sum "$temporary/$asset" | awk '{print $1}')
    fi
    [ "$actual" = "$expected" ] || fail 'Checksum mismatch; nothing was installed'
    tar -tzf "$temporary/$asset" > "$temporary/entries"
    awk '$0 !~ /^linter(\/|$)/ || $0 ~ /(^|\/)\.\.(\/|$)/ {exit 1}' \
        "$temporary/entries" || fail 'Unsafe archive paths'
    tar -tvzf "$temporary/$asset" | awk 'substr($0,1,1) !~ /[-d]/ {exit 1}' \
        || fail 'Archive contains links or unsupported entries'
    tar -xzf "$temporary/$asset" -C "$temporary"
    payload="$temporary/linter"
    for file in bin/linter bin/linter-mcp skills/software-design/SKILL.md \
        .codex-plugin/plugin.json .claude-plugin/plugin.json; do
        [ -f "$payload/$file" ] || fail "Incomplete release: $file"
    done
    [ -x "$payload/bin/linter" ] && [ -x "$payload/bin/linter-mcp" ] \
        || fail 'Release binaries are not executable'
    mkdir -p "$root"
    root=$(cd "$root" && pwd -P)
    destination="$root/plugins/linter"
    [ ! -L "$root/plugins" ] && [ ! -L "$destination" ] || fail 'Plugin path cannot be a symlink'
    if [ -e "$destination" ]; then
        [ -f "$destination/.linter-install" ] && \
            [ "$(cat "$destination/.linter-install")" = "$repo" ] \
            || fail "Refusing to replace an unrelated directory: $destination"
    fi
    cat > "$temporary/codex.json" <<'JSON'
{"name":"propdevil-linter","interface":{"displayName":"Propdevil Linter"},"plugins":[{"name":"linter","source":{"source":"local","path":"./plugins/linter"},"policy":{"installation":"AVAILABLE","authentication":"ON_INSTALL"},"category":"Productivity"}]}
JSON
    cat > "$temporary/claude.json" <<'JSON'
{"name":"propdevil-linter","owner":{"name":"Propdevil"},"plugins":[{"name":"linter","source":"./plugins/linter"}]}
JSON
    for catalog in codex claude; do
        case "$catalog" in
            codex) path="$root/.agents/plugins/marketplace.json" ;;
            claude) path="$root/.claude-plugin/marketplace.json" ;;
        esac
        if [ -e "$path" ]; then
            cmp -s "$path" "$temporary/$catalog.json" || fail "Unrelated marketplace: $path"
        fi
    done
    staging=$(mktemp -d "$root/.linter-install.XXXXXX")
    cp -R "$payload" "$staging/linter"
    printf '%s\n' "$repo" > "$staging/linter/.linter-install"
    printf '{"mcpServers":{"linter":{"command":"%s","args":[]}}}\n' \
        "$(json_string "$destination/bin/linter-mcp")" > "$staging/linter/.mcp.json"
    mkdir -p "$root/plugins" "$root/.agents/plugins" "$root/.claude-plugin"
    cp "$temporary/codex.json" "$root/.agents/plugins/marketplace.json"
    cp "$temporary/claude.json" "$root/.claude-plugin/marketplace.json"
    if [ -e "$destination" ]; then mv "$destination" "$staging/previous"; fi
    if ! mv "$staging/linter" "$destination"; then
        if [ -d "$staging/previous" ]; then mv "$staging/previous" "$destination"; fi
        fail 'Could not replace the installed bundle'
    fi
    # Registration failure keeps the verified bundle so the command can be retried.
    for candidate in $clients; do
        "$candidate" plugin marketplace add "$root" </dev/null
        if [ "$candidate" = codex ]; then
            codex plugin add linter@propdevil-linter </dev/null
        else
            claude plugin install linter@propdevil-linter --scope user </dev/null
            claude plugin update linter@propdevil-linter --scope user </dev/null
        fi
    done
    printf '\nInstalled the design skill and MCP server for:%s\n' " $clients"
    printf 'CLI: %s/bin/linter\nStart a new client session to use the plugin.\n' "$destination"
}

main "$@"
