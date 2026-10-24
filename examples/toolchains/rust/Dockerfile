FROM rust:1.99.1-slim AS dependencies
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential cmake pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /dependencies
COPY Cargo.toml Cargo.lock ./
# Only this trusted image build uses Cargo or downloads dependencies. Generated
# collectors compile directly with rustc against these immutable libraries.
RUN mkdir src \
    && printf 'fn main() {}\n' > src/main.rs \
    && cargo build --release --locked

FROM rust:1.99.1-slim
COPY --from=dependencies /dependencies/target/release/deps /opt/engarg-rust/deps
COPY --chmod=0755 build-collector.sh /usr/local/bin/build-collector
# Exercise the final image as the runner UID, with immutable toolchain files and
# no Cargo build tree or native search paths.
RUN mkdir /src /build \
    && printf '%s\n' \
        '#[derive(serde::Deserialize, serde::Serialize)] struct Probe { ok: bool }' \
        'fn main() {' \
        '    let _client = reqwest::blocking::Client::new();' \
        '    let _json = serde_json::to_string(&Probe { ok: true }).unwrap();' \
        '}' > /src/main.rs \
    && chown 65534:65534 /build
USER 65534:65534
WORKDIR /build
RUN --mount=type=bind,from=dependencies,source=/usr/local/rustup,target=/usr/local/rustup,readonly \
    --mount=type=bind,from=dependencies,source=/usr/local/cargo,target=/usr/local/cargo,readonly \
    --mount=type=bind,from=dependencies,source=/dependencies/target/release/deps,target=/opt/engarg-rust/deps,readonly \
    build-collector --extern serde --extern serde_json --extern reqwest \
        /src/main.rs -o /build/collector \
    && /build/collector \
    && rm /build/collector
USER root
WORKDIR /
RUN rm -rf /src /build
USER 65534:65534
