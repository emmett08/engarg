FROM rust:1.99.1-slim AS dependencies
WORKDIR /dependencies
COPY Cargo.toml Cargo.lock prepare-libraries.sh build-collector.sh smoke.rs ./
# Only this trusted image build downloads dependencies or runs Cargo. Keep the
# preparation in one layer: VFS-backed builders otherwise copy each large cache.
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export CARGO_HTTP_CAINFO=/run/secrets/proxy_ca; fi \
    && apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential cmake pkg-config ca-certificates jq \
    && mkdir src \
    && printf 'fn main() {}\n' > src/main.rs \
    && cargo build --release --locked --jobs 2 --message-format=json > /tmp/dependency-build.jsonl \
    && sh prepare-libraries.sh \
    && mkdir -p /prepared/opt /prepared/usr/local/bin /prepared/src /prepared/build \
    && mv /opt/engarg-rust /prepared/opt/engarg-rust \
    && mv target/release/deps /prepared/opt/engarg-rust/deps \
    && cp build-collector.sh /prepared/usr/local/bin/build-collector \
    && chmod 0755 /prepared/usr/local/bin/build-collector \
    && cp smoke.rs /prepared/src/main.rs \
    && chmod 0644 /prepared/src/main.rs \
    && chown 65534:65534 /prepared/build \
    && rm -rf target /usr/local/cargo/registry /var/lib/apt/lists/* /tmp/dependency-*

FROM rust:1.99.1-slim
COPY --from=dependencies /prepared/ /
# Exercise the final image as the runner UID, with immutable compiler/libraries
# and no Cargo build tree, writable package cache, native search path or network.
RUN --network=none \
    --mount=type=bind,from=dependencies,source=/usr/local/rustup,target=/usr/local/rustup,readonly \
    --mount=type=bind,from=dependencies,source=/usr/local/cargo,target=/usr/local/cargo,readonly \
    --mount=type=bind,from=dependencies,source=/prepared/opt/engarg-rust,target=/opt/engarg-rust,readonly \
    su --preserve-environment --shell /bin/sh nobody --command \
        'test "$(id -u):$(id -g)" = 65534:65534 \
        && build-collector --extern serde --extern serde_json --extern reqwest \
            --extern why_rs --extern polars --extern statrs --extern nalgebra --extern varisat \
            --extern linfa_nn --extern ndarray --extern good_lp \
            /src/main.rs -o /build/collector \
        && test "$(/build/collector)" = evidence-packages-ok' \
    && rm -rf /src /build
WORKDIR /
USER 65534:65534
