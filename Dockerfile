FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
ARG PACKAGE
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BUILD_TYPE=release

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --recipe-path recipe.json -p ${PACKAGE}

RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libpq-dev protobuf-compiler
COPY . .
RUN cargo build -p ${PACKAGE}

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y \
    libpq5
ARG PACKAGE
COPY --from=builder /app/target/debug/${PACKAGE} /usr/local/bin/app
ENTRYPOINT ["app"]
