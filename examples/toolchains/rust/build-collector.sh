#!/bin/sh
set -eu

# The profile exposes the fixed source, output and available dependencies.
# Generated source cannot choose compiler options or run a Cargo build script.
if [ "$#" -ne 9 ] || [ "$*" != '--extern serde --extern serde_json --extern reqwest /src/main.rs -o /build/collector' ]; then
    printf 'unsupported Rust collector build arguments\n' >&2
    exit 1
fi

dependencies=/opt/engarg-rust/deps
library() {
    crate=$1
    set -- "$dependencies/lib$crate-"*.rlib
    if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
        printf 'expected exactly one prebuilt %s library\n' "$crate" >&2
        exit 1
    fi
    printf '%s\n' "$1"
}
serde=$(library serde)
serde_json=$(library serde_json)
reqwest=$(library reqwest)

exec rustc --edition=2024 -C debuginfo=0 -C strip=symbols \
    -L "dependency=$dependencies" \
    --extern "serde=$serde" --extern "serde_json=$serde_json" \
    --extern "reqwest=$reqwest" /src/main.rs -o /build/collector
