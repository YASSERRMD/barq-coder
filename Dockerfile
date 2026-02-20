# Build stage
FROM rust:1.75-slim AS builder
WORKDIR /app
# Install dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev build-essential
# Copy manifests
COPY Cargo.toml Cargo.lock ./
# Copy source
COPY src ./src
# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y ca-certificates git curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/barqcoder /usr/local/bin/barqcoder

# Create a non-root user
RUN useradd -ms /bin/bash barqcoder
USER barqcoder

ENTRYPOINT ["barqcoder"]
