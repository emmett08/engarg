FROM golang:1.25.14-bookworm AS dependencies
# Freeze the complete pure-Go standard library during the trusted image build.
# Generated collectors do not resolve modules or build into a writable Go cache.
ENV CGO_ENABLED=0
RUN mkdir -p /opt/engarg-go/packages \
    && go list -export -f '{{if .Export}}{{.ImportPath}} {{.Export}}{{end}}' std > /tmp/exports \
    && while read -r package archive; do \
        test -n "$package" || continue; \
        destination="/opt/engarg-go/packages/$package.a"; \
        mkdir -p "$(dirname "$destination")"; \
        cp "$archive" "$destination"; \
        printf 'packagefile %s=%s\n' "$package" "$destination" >> /opt/engarg-go/importcfg; \
    done < /tmp/exports

FROM golang:1.25.14-bookworm
COPY --from=dependencies /opt/engarg-go /opt/engarg-go
COPY --chmod=0755 build-collector.sh /usr/local/bin/build-collector
# Check the exact single-file build and link interface under the runtime UID.
RUN mkdir /src /build \
    && printf '%s\n' 'package main' 'import("encoding/json";"fmt";"net/http")' \
       'func main() { _ = http.Client{}; b, _ := json.Marshal(true); fmt.Println(string(b)) }' > /src/main.go \
    && chown 65534:65534 /build
USER 65534:65534
WORKDIR /build
RUN build-collector /src/main.go -o /build/collector \
    && test "$(/build/collector)" = true \
    && rm /build/collector /build/collector.a
USER root
WORKDIR /
RUN rm -rf /src /build
USER 65534:65534
