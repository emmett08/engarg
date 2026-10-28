#!/bin/sh
set -eu

if [ "$#" -ne 3 ] || [ "$*" != '/src/main.go -o /build/collector' ]; then
    printf 'unsupported Go collector build arguments\n' >&2
    exit 1
fi

# All imports resolve to the immutable standard-library and evidence archives.
# There is no module resolution, package installation or writable build cache.
go tool compile -trimpath /src -importcfg /opt/engarg-go/importcfg \
    -p main -o /build/collector.a /src/main.go
exec go tool link -s -w -importcfg /opt/engarg-go/importcfg \
    -o /build/collector /build/collector.a
