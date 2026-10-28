# syntax=docker/dockerfile:1
# Both language images share one pinned dependency closure and preparation path.
FROM node:24-slim AS dependencies
COPY . /opt/engarg-typescript/
# Only this trusted build installs dependencies. The optional CA secret supports
# managed HTTPS proxies without disabling TLS verification or retaining the CA.
# Node ESM ignores NODE_PATH. A root ancestor link gives both module systems and
# tsc the same immutable packages from /src, /build, or /work, without writing to
# read-only source mounts or requiring a generated package.json.
RUN --mount=type=secret,id=proxy_ca \
    if [ -s /run/secrets/proxy_ca ]; then \
        export NODE_EXTRA_CA_CERTS=/run/secrets/proxy_ca; \
    fi \
    && cd /opt/engarg-typescript \
    && npm ci --ignore-scripts --no-audit --no-fund --strict-ssl=true \
    && npm cache clean --force \
    && chmod -R a+rX /opt/engarg-typescript \
    && ln -s /opt/engarg-typescript/node_modules /node_modules \
    && mkdir /src /build \
    && chmod 0555 /src \
    && chown 65534:65534 /build

FROM dependencies AS javascript
USER 65534:65534
WORKDIR /
# Keep this identical to typescript.json: strict compilation of the fixed entry
# followed by offline execution of the emitted collector under the runtime UID.
RUN --network=none \
    --mount=type=bind,from=dependencies,source=/opt/engarg-typescript/evidence-smoke.mjs,target=/src/collector.mjs,readonly \
    --mount=type=bind,from=dependencies,source=/opt/engarg-typescript/evidence-smoke.ts,target=/src/collector.ts,readonly \
    node --check /src/collector.mjs \
    && node /src/collector.mjs \
    && /opt/engarg-typescript/node_modules/.bin/tsc \
    --strict --esModuleInterop --noEmitOnError --target ES2022 \
    --module nodenext --moduleResolution nodenext \
    --types node,engarg-evidence \
    --typeRoots /opt/engarg-typescript/node_modules/@types,/opt/engarg-typescript/types \
    --outDir /build /src/collector.ts \
    && node /build/collector.js \
    && rm /build/collector.js

# Distinct public targets share the same tested runtime and compiler closure.
FROM javascript AS typescript
