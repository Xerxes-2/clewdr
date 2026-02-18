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
        export CXX="x86_64-linux-gnu-g++"
        ;; \
    "linux/arm64") \
        RUST_TARGET="aarch64-unknown-linux-musl"
        export CXX="aarch64-linux-gnu-g++"
        # Keep large-file symbols compatible with musl toolchain deps (boring/aws-lc).
        export CFLAGS="-D_FILE_OFFSET_BITS=64 -D_LARGEFILE64_SOURCE"
        export CXXFLAGS="${CFLAGS}"
        export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,--allow-shlib-undefined"
        ;; \
    *) echo "Unsupported architecture: ${TARGETPLATFORM}" >&2; exit 1 ;; \
esac
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
        export CXX="x86_64-linux-gnu-g++"
        ;; \
    "linux/arm64") \
        RUST_TARGET="aarch64-unknown-linux-musl"
        export CXX="aarch64-linux-gnu-g++"
        # Keep large-file symbols compatible with musl toolchain deps (boring/aws-lc).
        export CFLAGS="-D_FILE_OFFSET_BITS=64 -D_LARGEFILE64_SOURCE"
        export CXXFLAGS="${CFLAGS}"
        export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,--allow-shlib-undefined"
        ;; \
    *) echo "Unsupported architecture: ${TARGETPLATFORM}" >&2; exit 1 ;; \
esac
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
