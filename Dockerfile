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
# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    clang \
    libclang-dev \
    perl \
    pkg-config \
    upx-ucl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=planner /build/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer.
RUN mkdir -p ~/.cargo \
    && cargo chef cook --release --no-default-features --features embed-resource,xdg --recipe-path recipe.json

# Build application
COPY . .
COPY --from=frontend-builder /build/static/ ./static
RUN cargo build --release --no-default-features --features embed-resource,xdg --bin clewdr \
    && upx --best --lzma ./target/release/clewdr \
    && cp ./target/release/clewdr /build/clewdr \
    && mkdir -p /etc/clewdr/log \
    && touch /etc/clewdr/clewdr.toml

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libgcc-s1 \
    libstdc++6 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=backend-builder /build/clewdr /usr/local/bin/clewdr
COPY --from=backend-builder /etc/clewdr /etc/clewdr
ENV CLEWDR_IP=0.0.0.0
ENV CLEWDR_PORT=8484
ENV CLEWDR_CHECK_UPDATE=FALSE
ENV CLEWDR_AUTO_UPDATE=FALSE

EXPOSE 8484

VOLUME [ "/etc/clewdr" ]
CMD ["/usr/local/bin/clewdr", "--config", "/etc/clewdr/clewdr.toml", "--log-dir", "/etc/clewdr/log"]
