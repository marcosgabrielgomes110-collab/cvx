#!/usr/bin/env sh
# Verify downloaded release assets and their checksums.
set -eu

directory="${1:-.}"
cd "$directory"

found=0
for checksum in *.sha256; do
    [ -f "$checksum" ] || continue
    found=1
    shasum -a 256 -c "$checksum"
done

if [ "$found" -eq 0 ]; then
    echo "no .sha256 files found in $directory" >&2
    exit 1
fi

if command -v gpg >/dev/null 2>&1; then
    for signature in *.asc; do
        [ -f "$signature" ] || continue
        gpg --verify "$signature" "${signature%.asc}"
    done
fi

echo "all release checksums verified in $directory"
