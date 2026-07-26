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
COPY xtask/ xtask/
COPY .cargo/ .cargo/
RUN cargo binstall trunk --no-confirm && \
    cd clewdr-frontend && trunk build --release

FROM docker.io/lukemathwalker/cargo-chef:latest-rust-trixie AS chef
WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS backend-builder
ARG TARGETARCH
# Overridable so a mirror can be used if musl.cc is unreachable.
ARG MUSL_TOOLCHAIN_BASE=https://musl.cc

# clang/libclang stay for bindgen, which loads libclang at runtime. That is
# also why this stage keeps a glibc host: on a musl host the build scripts are
# statically linked and cannot dlopen at all.
RUN apt-get update && apt-get install -y \
    build-essential \
    cmake \
    clang \
    libclang-dev \
    perl \
    pkg-config \
    git \
    upx-ucl \
    && rm -rf /var/lib/apt/lists/*

# Determine musl target from Docker platform, and fetch a musl toolchain that
# can compile C++.
#
# Debian's musl-tools only provides musl-gcc, with no C++ counterpart, but
# btls-sys vendors BoringSSL, which is C++. Pairing musl-gcc with the system
# g++ compiles but cannot link: glibc's libstdc++.a pulls in __isoc23_strtoul
# and __libc_single_threaded, which musl does not define. Pairing it with
# clang++ does not even configure, because BoringSSL infers the compiler
# family from CXX and then feeds clang-only flags to a GCC CC.
#
# The -native tarballs are musl-hosted and statically linked, so they run on
# this glibc image, and each arch fetches the toolchain for its own host --
# these images are built natively per architecture, not cross-compiled.
RUN case "$TARGETARCH" in \
    amd64) RUST_TARGET=x86_64-unknown-linux-musl; MUSL=x86_64-linux-musl-native ;; \
    arm64) RUST_TARGET=aarch64-unknown-linux-musl; MUSL=aarch64-linux-musl-native ;; \
    *) echo "Unsupported arch: $TARGETARCH" && exit 1 ;; \
    esac && \
    echo "$RUST_TARGET" > /tmp/rust-target && \
    echo "$MUSL" > /tmp/musl-toolchain && \
    rustup target add "$RUST_TARGET" && \
    curl -fsSL --retry 3 --retry-delay 5 "$MUSL_TOOLCHAIN_BASE/$MUSL.tgz" \
    | tar xz -C /opt && \
    "/opt/$MUSL/bin/g++" --version | head -1

COPY --from=planner /build/recipe.json recipe.json

# RUSTFLAGS only reaches target units while --target is set, so build scripts
# keep using the host compiler and bindgen keeps working. It must match the
# application build below exactly, or that build cannot reuse this layer.

# Build dependencies - this is the caching Docker layer.
RUN RUST_TARGET=$(cat /tmp/rust-target) && MUSL=$(cat /tmp/musl-toolchain) && \
    CC="/opt/$MUSL/bin/gcc" CXX="/opt/$MUSL/bin/g++" \
    RUSTFLAGS="-C linker=/opt/$MUSL/bin/gcc" \
    cargo chef cook --release --target "$RUST_TARGET" \
    --no-default-features --features embed-resource,xdg \
    --recipe-path recipe.json

# Build application
COPY . .
COPY --from=frontend-builder /build/static/ ./static
RUN RUST_TARGET=$(cat /tmp/rust-target) && MUSL=$(cat /tmp/musl-toolchain) && \
    CC="/opt/$MUSL/bin/gcc" CXX="/opt/$MUSL/bin/g++" \
    RUSTFLAGS="-C linker=/opt/$MUSL/bin/gcc" \
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
