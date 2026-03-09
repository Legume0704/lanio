# Build stage
FROM rust:1.93-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    wget \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Copy the Rust binary from builder
COPY --from=builder /build/target/release/lanio /usr/local/bin/

EXPOSE 8078

HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD wget -qO- http://localhost:8078/health || exit 1

CMD [ "/usr/local/bin/lanio" ]
