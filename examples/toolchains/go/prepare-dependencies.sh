#!/bin/sh
set -eu

# Trusted image-build adapter. The final runtime receives only immutable Go
# archives, their import map and package pins, never this module/build cache.
if [ -f /run/secrets/proxy_ca ]; then
    export SSL_CERT_FILE=/run/secrets/proxy_ca
fi
cp go.mod /tmp/locked.mod
cp go.sum /tmp/locked.sum
go mod download
go mod verify
cmp go.mod /tmp/locked.mod
cmp go.sum /tmp/locked.sum

destination=/opt/engarg-go
mkdir -p "$destination/packages"
go list -p=2 -mod=readonly -deps -export \
    -f '{{if .Export}}{{.ImportPath}} {{.Export}}{{end}}' \
    std $(cat packages.txt) > /tmp/exports
while read -r package archive; do
    test -n "$package" || continue
    prepared="$destination/packages/$package.a"
    mkdir -p "$(dirname "$prepared")"
    cp "$archive" "$prepared"
    printf 'packagefile %s=%s\n' "$package" "$prepared" >> "$destination/importcfg"
done < /tmp/exports
cp go.mod go.sum packages.txt build-collector.sh smoke.go "$destination/"
chmod 0644 "$destination/go.mod" "$destination/go.sum" \
    "$destination/packages.txt" "$destination/build-collector.sh" "$destination/smoke.go"
rm -rf /go/pkg/mod /root/.cache/go-build
