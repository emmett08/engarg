#!/bin/sh
set -eu

if [ "$#" -ne 3 ] || [ "$*" != '/src/main.go -o /build/collector' ]; then
    printf 'unsupported Go collector build arguments\n' >&2
    exit 1
fi

# All imports must resolve to the immutable standard-library archive set.
go tool compile -trimpath /src -importcfg /opt/engarg-go/importcfg \
    -p main -o /build/collector.a /src/main.go
exec go tool link -s -w -importcfg /opt/engarg-go/importcfg \
    -o /build/collector /build/collector.a
