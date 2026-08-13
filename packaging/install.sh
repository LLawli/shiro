#!/bin/sh
# shiro installer, the primary install path:
#
#   curl -fsSL https://raw.githubusercontent.com/LLawli/shiro/main/packaging/install.sh | sh
#
# Downloads the release tarball for this architecture, verifies its sha256
# against the published checksum, and installs to ~/.local (no root; survives
# OS image rebases on atomic distros). Override the destination with
# PREFIX=/some/path.
#
# The repository is private for now, so a read token is required until the
# first public release: pass it as SHIRO_TOKEN or GITHUB_TOKEN.
#
# Requires only curl + tar + sha256sum, deliberately not cargo or git, since
# the whole point of shipping a binary is that the host needs neither.
set -eu

REPO="${SHIRO_REPO:-LLawli/shiro}"
PREFIX="${PREFIX:-$HOME/.local}"
TOKEN="${SHIRO_TOKEN:-${GITHUB_TOKEN:-}}"
API="https://api.github.com/repos/$REPO"

command -v curl >/dev/null 2>&1 || { echo "error: curl is required" >&2; exit 1; }
command -v tar >/dev/null 2>&1 || { echo "error: tar is required" >&2; exit 1; }

case "$(uname -m)" in
    x86_64) TARGET="x86_64-unknown-linux-musl" ;;
    *) echo "error: no release is built for $(uname -m); build from source with 'cargo build --release'" >&2; exit 1 ;;
esac

# One place decides whether the token is sent, so a private repository and a
# public one differ by an environment variable and nothing else.
fetch() {
    if [ -n "$TOKEN" ]; then
        curl -fsSL -H "Authorization: Bearer $TOKEN" "$@"
    else
        curl -fsSL "$@"
    fi
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "==> Finding the latest release of $REPO ..."
release=$(fetch "$API/releases/latest" 2>/dev/null) || release=""
TAG=$(printf '%s' "$release" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)

if [ -z "$TAG" ]; then
    echo "error: no release found for $REPO." >&2
    if [ -z "$TOKEN" ]; then
        echo "       The repository is private; set SHIRO_TOKEN to a read token." >&2
    fi
    exit 1
fi

VER=${TAG#v}
ASSET="shiro-$VER-$TARGET.tar.gz"

# A private release asset cannot be fetched from its browser URL: it is served
# by asset id, from the API, with an octet-stream Accept header.
asset_url() {
    name=$1
    if [ -z "$TOKEN" ]; then
        echo "https://github.com/$REPO/releases/download/$TAG/$name"
        return
    fi
    id=$(printf '%s' "$release" | tr -d '\n' | tr '{' '\n' |
        grep "\"name\"[[:space:]]*:[[:space:]]*\"$name\"" |
        sed -n 's/.*"id"[[:space:]]*:[[:space:]]*\([0-9]*\).*/\1/p' | head -1)
    [ -n "$id" ] || { echo "error: $name is not among the assets of $TAG" >&2; exit 1; }
    echo "$API/releases/assets/$id"
}

echo "==> Downloading $ASSET ($TAG) ..."
fetch -H "Accept: application/octet-stream" -o "$tmp/$ASSET" "$(asset_url "$ASSET")"

echo "==> Verifying checksum ..."
if command -v sha256sum >/dev/null 2>&1; then
    fetch -H "Accept: application/octet-stream" -o "$tmp/$ASSET.sha256" "$(asset_url "$ASSET.sha256")"
    (cd "$tmp" && sha256sum -c "$ASSET.sha256")
else
    echo "warning: sha256sum not found; skipping checksum verification" >&2
fi

tar -xzf "$tmp/$ASSET" -C "$tmp"
[ -s "$tmp/shiro" ] || { echo "error: the tarball contains no shiro binary" >&2; exit 1; }

echo "==> Installing to $PREFIX ..."
install -D -m 0755 "$tmp/shiro" "$PREFIX/bin/shiro"

case ":${PATH}:" in
    *":$PREFIX/bin:"*) ;;
    *) echo "note: add $PREFIX/bin to your PATH" ;;
esac

echo "Done. Next steps:"
echo "  shiro doctor"
echo "  shiro install"
