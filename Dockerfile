# Vergil worker container — Phase 4 Slice C3.
#
# Multi-stage build:
#   * `deps` stage installs the heavy subprocess dependencies (solc,
#     halmos, slither, foundry, z3, cvc5) once.
#   * `vergil` stage cargo-builds the Rust workspace (vergil + vergilbench
#     + kill-criterion binaries).
#   * `runtime` final stage combines them, sized for a worker pool.
#
# DO NOT push to a public registry — Phase 4 stays internal per the
# proprietary posture. V2 picks up registry + tagging when it deploys.
#
# Build locally:
#   docker build -t vergil-worker .
#
# Smoke test:
#   docker run --rm vergil-worker vergil verify /workspace/examples/erc20

# ─── deps stage ────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS deps

RUN apt-get update && apt-get install -y \
        ca-certificates \
        curl \
        git \
        python3 \
        python3-pip \
        python3-venv \
        z3 \
        libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Install pipx for halmos/slither isolation. The PATH gets prepended
# below so `halmos` and `slither` resolve in the final stage.
RUN python3 -m pip install --break-system-packages pipx \
    && pipx ensurepath \
    && pipx install halmos==0.3.3 \
    && pipx install slither-analyzer==0.11.0

# Install Foundry (forge + cast + anvil). Pinned via FOUNDRY_RELEASE.
ENV FOUNDRY_RELEASE=v1.0.0
RUN curl -L https://foundry.paradigm.xyz | bash \
    && /root/.foundry/bin/foundryup --version stable

# Install cvc5 (z3 came from apt; cvc5 from upstream releases).
ARG CVC5_VERSION=1.2.1
RUN ARCH=$(dpkg --print-architecture | sed 's/amd64/x86_64/; s/arm64/aarch64/') \
    && curl -L "https://github.com/cvc5/cvc5/releases/download/cvc5-${CVC5_VERSION}/cvc5-Linux-${ARCH}-static.zip" -o /tmp/cvc5.zip \
    && cd /tmp && unzip cvc5.zip \
    && mv "cvc5-Linux-${ARCH}-static/bin/cvc5" /usr/local/bin/cvc5 \
    && chmod +x /usr/local/bin/cvc5 \
    && rm -rf /tmp/cvc5.zip "/tmp/cvc5-Linux-${ARCH}-static"

# Install solc (pinned to 0.8.20 matching examples/*/foundry.toml).
ARG SOLC_VERSION=v0.8.20
RUN ARCH=$(dpkg --print-architecture | sed 's/amd64/static-linux/; s/arm64/static-linux/') \
    && curl -L "https://github.com/ethereum/solidity/releases/download/${SOLC_VERSION}/solc-${ARCH}" -o /usr/local/bin/solc \
    && chmod +x /usr/local/bin/solc

# ─── vergil build stage ────────────────────────────────────────────────
FROM rust:1.85-slim AS vergil-build

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
# Copy manifest first for layer caching. .dockerignore excludes target/.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY vergilbench ./vergilbench
RUN cargo build --release --bin vergil --bin vergilbench --bin kill-criterion

# ─── runtime stage ─────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
        ca-certificates \
        python3 \
        z3 \
        libssl3 \
        git \
    && rm -rf /var/lib/apt/lists/*

# Bring the heavy deps + binaries over from the build stages.
COPY --from=deps /usr/local/bin/cvc5 /usr/local/bin/cvc5
COPY --from=deps /usr/local/bin/solc /usr/local/bin/solc
COPY --from=deps /root/.foundry/bin/forge /usr/local/bin/forge
COPY --from=deps /root/.foundry/bin/cast /usr/local/bin/cast
COPY --from=deps /root/.foundry/bin/anvil /usr/local/bin/anvil
COPY --from=deps /root/.local/pipx /root/.local/pipx
ENV PATH="/root/.local/bin:${PATH}"
# pipx installs symlinks under ~/.local/bin; create them in the
# runtime root user's home.
RUN mkdir -p /root/.local/bin \
    && ln -sf /root/.local/pipx/venvs/halmos/bin/halmos /root/.local/bin/halmos \
    && ln -sf /root/.local/pipx/venvs/slither-analyzer/bin/slither /root/.local/bin/slither

# Vergil binaries.
COPY --from=vergil-build /build/target/release/vergil /usr/local/bin/vergil
COPY --from=vergil-build /build/target/release/vergilbench /usr/local/bin/vergilbench
COPY --from=vergil-build /build/target/release/kill-criterion /usr/local/bin/kill-criterion

# Bring the example contracts + property catalog into the image so the
# in-container smoke test can run without mounting anything.
WORKDIR /workspace
COPY examples ./examples
COPY crates/vergil-properties/templates ./crates/vergil-properties/templates
COPY vergilbench/contracts ./vergilbench/contracts
COPY vergilbench/NOTICE ./vergilbench/NOTICE

# Halmos + foundry need a writable HOME for caches.
ENV HOME=/root

# Healthcheck: every required binary is present + responds to --version.
RUN solc --version >/dev/null \
    && forge --version >/dev/null \
    && halmos --version >/dev/null \
    && slither --version >/dev/null \
    && z3 --version >/dev/null \
    && cvc5 --version >/dev/null \
    && vergil --version >/dev/null

# Default command runs the doctor — operators see immediately whether
# the image is healthy.
ENTRYPOINT ["/usr/local/bin/vergil"]
CMD ["doctor"]
