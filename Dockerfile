FROM node:lts-slim AS frontend-builder
WORKDIR /build/frontend
RUN npm install -g pnpm
COPY frontend/ .
RUN pnpm install && pnpm run build

FROM docker.io/lukemathwalker/cargo-chef:latest-rust-trixie AS chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS backend-builder
ARG TARGETPLATFORM
# Install musl target and required dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    musl-tools \
    musl-dev \
    cmake \
    clang \
    libclang-dev \
    perl \
    pkg-config \
    upx-ucl \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl && \
    rustup target add aarch64-unknown-linux-musl
COPY --from=planner /build/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer!
RUN <<EOF
set -e
case ${TARGETPLATFORM} in \
    "linux/amd64") \
        RUST_TARGET="x86_64-unknown-linux-musl"
        MUSL_TRIPLE="x86_64-linux-musl"
        TARGET_ENV="x86_64_unknown_linux_musl"
        ;; \
    "linux/arm64") \
        RUST_TARGET="aarch64-unknown-linux-musl"
        MUSL_TRIPLE="aarch64-linux-musl"
        TARGET_ENV="aarch64_unknown_linux_musl"
        ;; \
    *) echo "Unsupported architecture: ${TARGETPLATFORM}" >&2; exit 1 ;; \
esac

if command -v "${MUSL_TRIPLE}-gcc" >/dev/null 2>&1; then
    export CC="${MUSL_TRIPLE}-gcc"
elif command -v musl-gcc >/dev/null 2>&1; then
    export CC="musl-gcc"
else
    echo "No musl C compiler found for ${MUSL_TRIPLE}" >&2
    exit 1
fi

if command -v "${MUSL_TRIPLE}-g++" >/dev/null 2>&1; then
    export CXX="${MUSL_TRIPLE}-g++"
elif command -v musl-g++ >/dev/null 2>&1; then
    export CXX="musl-g++"
else
    # Most deps are C-only; fall back to CC when musl g++ wrapper is missing.
    export CXX="${CC}"
fi

export "CC_${TARGET_ENV}=${CC}"
export "CXX_${TARGET_ENV}=${CXX}"
export "CARGO_TARGET_${TARGET_ENV}_LINKER=${CC}"

mkdir -p ~/.cargo
cargo chef cook --release --target ${RUST_TARGET} --no-default-features --features embed-resource,xdg --recipe-path recipe.json
EOF

# Build application
COPY . .
ENV RUSTFLAGS="-Awarnings"
COPY --from=frontend-builder /build/static/ ./static
RUN <<EOF
set -e
case ${TARGETPLATFORM} in \
    "linux/amd64") \
        RUST_TARGET="x86_64-unknown-linux-musl"
        MUSL_TRIPLE="x86_64-linux-musl"
        TARGET_ENV="x86_64_unknown_linux_musl"
        ;; \
    "linux/arm64") \
        RUST_TARGET="aarch64-unknown-linux-musl"
        MUSL_TRIPLE="aarch64-linux-musl"
        TARGET_ENV="aarch64_unknown_linux_musl"
        ;; \
    *) echo "Unsupported architecture: ${TARGETPLATFORM}" >&2; exit 1 ;; \
esac

if command -v "${MUSL_TRIPLE}-gcc" >/dev/null 2>&1; then
    export CC="${MUSL_TRIPLE}-gcc"
elif command -v musl-gcc >/dev/null 2>&1; then
    export CC="musl-gcc"
else
    echo "No musl C compiler found for ${MUSL_TRIPLE}" >&2
    exit 1
fi

if command -v "${MUSL_TRIPLE}-g++" >/dev/null 2>&1; then
    export CXX="${MUSL_TRIPLE}-g++"
elif command -v musl-g++ >/dev/null 2>&1; then
    export CXX="musl-g++"
else
    export CXX="${CC}"
fi

export "CC_${TARGET_ENV}=${CC}"
export "CXX_${TARGET_ENV}=${CXX}"
export "CARGO_TARGET_${TARGET_ENV}_LINKER=${CC}"

cargo build --release --target ${RUST_TARGET}  --no-default-features --features embed-resource,xdg --bin clewdr
upx --best --lzma ./target/${RUST_TARGET}/release/clewdr
cp ./target/${RUST_TARGET}/release/clewdr /build/clewdr
mkdir -p /etc/clewdr && cd /etc/clewdr
touch clewdr.toml && mkdir -p log
EOF

FROM gcr.io/distroless/static
COPY --from=backend-builder /build/clewdr /usr/local/bin/clewdr
COPY --from=backend-builder /etc/clewdr /etc/
ENV CLEWDR_IP=0.0.0.0
ENV CLEWDR_PORT=8484
ENV CLEWDR_CHECK_UPDATE=FALSE
ENV CLEWDR_AUTO_UPDATE=FALSE

EXPOSE 8484

VOLUME [ "/etc/clewdr" ]
CMD ["/usr/local/bin/clewdr", "--config", "/etc/clewdr/clewdr.toml", "--log-dir", "/etc/clewdr/log"]
