#!/bin/sh
set -eu

# Trusted image-build adapter: map each exact direct Cargo dependency to its
# immutable compiler artifact. Different transitive versions remain in deps/.
destination=/opt/engarg-rust
mkdir -p "$destination/externs"
cargo metadata --locked --offline --no-deps --format-version 1 > /tmp/dependency-metadata.json
jq -r '.packages[0].dependencies[] | select(.kind == null) |
    [.name, ((.rename // .name) | gsub("-"; "_")), .req] | @tsv' \
    /tmp/dependency-metadata.json > /tmp/direct-dependencies.tsv
while read -r package crate requirement; do
    case "$requirement" in
        =*) version=${requirement#=} ;;
        *) printf 'direct dependency %s needs an exact version\n' "$package" >&2; exit 1 ;;
    esac
    archive=$(jq -ers --arg identity "#$package@$version" --arg crate "$crate" '
        [.[] | select(.reason == "compiler-artifact") |
            select(.package_id | endswith($identity)) |
            select(.target.name == $crate) | .filenames[] |
            select(endswith(".rlib"))] |
        if length == 1 then .[0] else error("ambiguous or absent direct library artifact") end
    ' /tmp/dependency-build.jsonl)
    test -f "$archive"
    ln -s "../deps/$(basename "$archive")" "$destination/externs/lib$crate.rlib"
done < /tmp/direct-dependencies.tsv
