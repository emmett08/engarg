# syntax=docker/dockerfile:1
# The requirements lock targets CPython 3.13 on Linux x86_64.
FROM python:3.13-slim@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb
WORKDIR /opt/engarg-evidence
COPY requirements.lock ./
# Only this operator-owned trusted build may download/install dependencies.
# A managed environment can provide its public proxy CA as an optional secret.
RUN --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export PIP_CERT=/run/secrets/proxy_ca; fi; \
    python -m pip install --disable-pip-version-check --no-cache-dir \
      --only-binary=:all: --require-hashes -r requirements.lock \
    && python -m pip check \
    && python -I -B -c "from importlib.metadata import version; import dowhy, causallearn; assert version('dowhy') == '0.14'; assert version('causal-learn') == '0.1.4.8'"
ENV OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 MPLCONFIGDIR=/tmp/engarg-matplotlib
USER 65534:65534
WORKDIR /
