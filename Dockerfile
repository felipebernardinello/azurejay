FROM rust:1.98-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo fetch

COPY . .
RUN cargo build --release --bin azurejay

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/azurejay /usr/local/bin/azurejay
COPY --from=builder /app/migrations /app/migrations
EXPOSE 8000
ENTRYPOINT ["azurejay"]
