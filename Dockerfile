FROM rust:1.85-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY lib ./lib
# Place pre-built Solidity artifact where rain.math.float expects it
COPY artifacts/DecimalFloat.json lib/rain.math.float/out/DecimalFloat.sol/DecimalFloat.json
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/rain-oracle-server /usr/local/bin/
ENTRYPOINT ["rain-oracle-server"]
