# Declared before the first FROM so it can select the builder image tag below.
ARG TARGETARCH

FROM docker.io/lukemathwalker/cargo-chef:latest-rust-trixie AS frontend-builder
WORKDIR /build
RUN rustup target add wasm32-unknown-unknown && \
    curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
# Dummy src to satisfy workspace root member
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
COPY Cargo.toml Cargo.lock ./
COPY clewdr-types/ clewdr-types/
COPY clewdr-frontend/ clewdr-frontend/
# Not used by this stage, but cargo refuses to read the workspace while any
# member manifest is missing.
COPY anthropic-wire/ anthropic-wire/
COPY xtask/ xtask/
COPY .cargo/ .cargo/
RUN cargo binstall trunk --no-confirm && \
    cd clewdr-frontend && trunk build --release

FROM docker.io/lukemathwalker/cargo-chef:latest-rust-trixie AS chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# wreq links BoringSSL, which is partly C++, so cross-compiling it needs a musl
# C++ compiler and a musl libstdc++.a. Debian's musl-tools has neither: it ships
# musl-gcc (a shell wrapper around the host gcc) and nothing else, so the C++
# probe CMake runs fails and reports it as "Could NOT find Threads".
#
# These images carry a full GCC cross toolchain built with musl-cross-make
# (--enable-languages=c,c++), and preset CARGO_BUILD_TARGET, TARGET_CC and
# TARGET_CXX, so no compiler variables need to be passed here. The tag alias
# matches Docker's TARGETARCH, so the arch mapping is the tag itself.
FROM ghcr.io/rust-cross/rust-musl-cross:${TARGETARCH}-musl AS backend-builder
WORKDIR /build

# upx is not in the image; cargo-chef is only needed for `cook` below.
RUN apt-get update && apt-get install -y --no-install-recommends upx-ucl \
    && rm -rf /var/lib/apt/lists/* \
    && curl -L --proto '=https' --tlsv1.2 -sSf \
       https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash \
    && cargo binstall cargo-chef --no-confirm

COPY --from=planner /build/recipe.json recipe.json

# Build dependencies - this is the caching Docker layer.
# --bin clewdr matches the build below, so the frontend's native dependencies
# are not cooked here. Its wasm build happens in the frontend stage.
RUN cargo chef cook --release --bin clewdr \
    --no-default-features --features embed-resource,xdg \
    --recipe-path recipe.json

# Build application
COPY . .
COPY --from=frontend-builder /build/static/ ./static
RUN cargo build --release \
    --no-default-features --features embed-resource,xdg --bin clewdr \
    && cp "./target/${RUST_MUSL_CROSS_TARGET}/release/clewdr" /build/clewdr \
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
