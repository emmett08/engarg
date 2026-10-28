#!/bin/sh
set -eu

# The profile exposes the fixed source, output and available dependencies.
# Generated source cannot choose compiler options or run a Cargo build script.
if [ "$#" -ne 25 ] || [ "$*" != '--extern serde --extern serde_json --extern reqwest --extern why_rs --extern polars --extern statrs --extern nalgebra --extern varisat --extern linfa_nn --extern ndarray --extern good_lp /src/main.rs -o /build/collector' ]; then
    printf 'unsupported Rust collector build arguments\n' >&2
    exit 1
fi

set -- rustc --edition=2024 -C debuginfo=0 -C strip=symbols \
    -L dependency=/opt/engarg-rust/deps
for crate in serde serde_json reqwest why_rs polars statrs nalgebra varisat linfa_nn ndarray good_lp; do
    library="/opt/engarg-rust/externs/lib$crate.rlib"
    if [ ! -f "$library" ]; then
        printf 'missing pinned direct %s library\n' "$crate" >&2
        exit 1
    fi
    set -- "$@" --extern "$crate=$library"
done
exec "$@" /src/main.rs -o /build/collector
