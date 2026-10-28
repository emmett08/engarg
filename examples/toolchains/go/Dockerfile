FROM golang:1.25.14-alpine AS dependencies
# Freeze the standard library and selected pure-Go evidence APIs during the
# trusted image build. Generated collectors never resolve or download modules.
ENV CGO_ENABLED=0
ENV GOTOOLCHAIN=local
WORKDIR /dependencies
COPY go.mod go.sum packages.txt smoke.go build-collector.sh prepare-dependencies.sh ./
RUN --mount=type=secret,id=proxy_ca \
    sh prepare-dependencies.sh

FROM golang:1.25.14-alpine
ENV CGO_ENABLED=0 GOTOOLCHAIN=local
COPY --from=dependencies /opt/engarg-go /opt/engarg-go
# Check actual evidence APIs through the exact offline compile/link interface.
RUN --network=none \
    cp /opt/engarg-go/build-collector.sh /usr/local/bin/build-collector \
    && chmod 0755 /usr/local/bin/build-collector \
    && mkdir /src /build \
    && cp /opt/engarg-go/smoke.go /src/main.go \
    && chmod 0644 /src/main.go \
    && chown 65534:65534 /build \
    && su -s /bin/sh -c 'export PATH=/usr/local/go/bin:/usr/local/bin:/usr/bin:/bin; \
        test "$(id -u)" = 65534 && test "$(id -g)" = 65534 && cd /build \
        && build-collector /src/main.go -o /build/collector \
        && test "$(/build/collector)" = evidence-packages-ok' nobody \
    && rm -rf /src /build \
    && rm /opt/engarg-go/build-collector.sh /opt/engarg-go/smoke.go
USER 65534:65534
WORKDIR /
