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
ARG TARGETARCH

# Install build dependencies + musl toolchain
RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    clang \
    libclang-dev \
    perl \
    pkg-config \
    musl-tools \
    upx-ucl \
    && rm -rf /var/lib/apt/lists/*

# Determine musl target from Docker platform
RUN case "$TARGETARCH" in \
    amd64) echo "x86_64-unknown-linux-musl" > /tmp/rust-target ;; \
    arm64) echo "aarch64-unknown-linux-musl" > /tmp/rust-target ;; \
    *) echo "Unsupported arch: $TARGETARCH" && exit 1 ;; \
    esac && \
    rustup target add "$(cat /tmp/rust-target)"

COPY --from=planner /build/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer.
RUN RUST_TARGET=$(cat /tmp/rust-target) && \
    CC=musl-gcc CXX=clang++ \
    cargo chef cook --release --target "$RUST_TARGET" \
    --no-default-features --features embed-resource,xdg \
    --recipe-path recipe.json

# Build application
COPY . .
COPY --from=frontend-builder /build/static/ ./static
RUN RUST_TARGET=$(cat /tmp/rust-target) && \
    CC=musl-gcc CXX=clang++ \
    cargo build --release --target "$RUST_TARGET" \
    --no-default-features --features embed-resource,xdg --bin clewdr \
    && cp ./target/"$RUST_TARGET"/release/clewdr /build/clewdr \
    && upx --best --lzma /build/clewdr \
    && mkdir -p /etc/clewdr/log \
    && touch /etc/clewdr/clewdr.toml

FROM gcr.io/distroless/static-debian13
COPY --from=backend-builder /build/clewdr /usr/local/bin/clewdr
COPY --from=backend-builder /etc/clewdr /etc/clewdr
ENV CLEWDR_IP=0.0.0.0
ENV CLEWDR_PORT=8484
ENV CLEWDR_CHECK_UPDATE=FALSE
ENV CLEWDR_AUTO_UPDATE=FALSE

EXPOSE 8484

VOLUME [ "/etc/clewdr" ]
CMD ["/usr/local/bin/clewdr", "--config", "/etc/clewdr/clewdr.toml", "--log-dir", "/etc/clewdr/log"]
