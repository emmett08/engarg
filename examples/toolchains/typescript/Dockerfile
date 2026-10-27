FROM node:24-slim
WORKDIR /opt/engarg-typescript
COPY package.json package-lock.json ./
# Network access and dependency installation occur only in this trusted build.
RUN npm ci --ignore-scripts --no-audit --no-fund \
    && npm cache clean --force
# Generated source compiles with these immutable compiler and Node declarations.
USER 65534:65534
WORKDIR /
