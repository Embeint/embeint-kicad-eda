#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Verify every staged input before anything is installed. Runs in the pinned base
# image, which provides only POSIX sh, gpgv, coreutils, gzip and mawk.
#
# Chain: base image archive keyring -> InRelease signature -> Packages.gz SHA-256
# -> (Filename, SHA-256, Package, Version, Architecture) of each staged .deb.
# Rust: the release key in the lock -> channel manifest signature -> tarball hashes.
set -eu

inputs=${1:?usage: verify_chain.sh INPUTS_DIR}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$inputs"
. ./snapshot.env

echo "$KEYRING_SHA256  $KEYRING" | sha256sum -c --quiet -
sha256sum -c --quiet inrelease.sha256

: > "$work/index"
for suite in $SUITES; do
    # Hashes are read only from the verified signed text that gpgv writes out.
    gpgv --quiet --keyring "$KEYRING" --output "$work/Release-$suite" "InRelease-$suite"
    for component in $COMPONENTS; do
        path="$component/binary-$ARCH/Packages.gz"
        expected=$(awk -v p="$path" '/^SHA256:/ { s = 1; next } /^[^ ]/ { s = 0 }
                   s && $3 == p { print $1 }' "$work/Release-$suite")
        if [ -z "$expected" ]; then
            echo "No signed hash for $suite $path" >&2
            exit 1
        fi
        # Copy once, then hash and decompress the same private bytes; a separate
        # gzip step (not a pipeline) makes any decompression error fatal.
        cp "Packages-$suite-$component.gz" "$work/Packages.gz"
        (cd "$work" && echo "$expected  Packages.gz" | sha256sum -c --quiet -)
        gzip -dc "$work/Packages.gz" > "$work/Packages"
        awk 'BEGIN { RS = ""; FS = "\n" }
            {
                f = h = p = v = a = ""
                for (i = 1; i <= NF; i++) {
                    if ($i ~ /^Filename: /) f = substr($i, 11)
                    else if ($i ~ /^SHA256: /) h = substr($i, 9)
                    else if ($i ~ /^Package: /) p = substr($i, 10)
                    else if ($i ~ /^Version: /) v = substr($i, 10)
                    else if ($i ~ /^Architecture: /) a = substr($i, 15)
                }
                if (f != "") print f " " h " " p " " v " " a
            }' "$work/Packages" >> "$work/index"
    done
done

count=0
while read -r filename sha name version arch; do
    if ! grep -qxF "$filename $sha $name $version $arch" "$work/index"; then
        echo "Not in the signed snapshot index: $filename $name $version" >&2
        exit 1
    fi
    echo "$sha  debs/${filename##*/}" | sha256sum -c --quiet -
    count=$((count + 1))
done < debs.list
staged=$(find debs -type f | wc -l)
if [ "$staged" -ne "$count" ]; then
    echo "Staged .deb files ($staged) differ from the locked list ($count)" >&2
    exit 1
fi

cd rust
sha256sum -c --quiet ../rust.sha256
gpgv --quiet --keyring "$inputs/rust/rust-key.gpg" channel.toml.asc channel.toml
# Every locked tarball hash must be named by the signed channel manifest.
tail -n +4 ../rust.sha256 | while read -r sha _name; do
    if ! grep -qF "xz_hash = \"$sha\"" channel.toml; then
        echo "Rust tarball hash not in the signed manifest: $sha" >&2
        exit 1
    fi
done
echo "Verified $count packages against signed snapshot indices and Rust tarballs against the signed manifest"
