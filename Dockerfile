FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
ARG PACKAGE
ARG BINARY
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json --bin ${BINARY}

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json -p ${PACKAGE} --bin ${BINARY}

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libpq-dev protobuf-compiler
COPY . .
RUN cargo build -p ${PACKAGE} --bin ${BINARY}

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y \
    libpq5
ARG BINARY
COPY --from=builder /app/target/debug/${BINARY} /usr/local/bin/app
CMD ["app"]
