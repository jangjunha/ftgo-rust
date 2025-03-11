FROM rust:1-slim-bookworm AS builder
RUN apt-get update && apt-get install -y \
    pkg-config libssl-dev libpq-dev protobuf-compiler

ARG PACKAGE
ARG BINARY
WORKDIR /app
COPY . .
RUN cargo build -r -p ${PACKAGE} --bin ${BINARY}

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    libpq5
ARG BINARY
COPY --from=builder /app/target/release/${BINARY} /usr/local/bin/app
CMD app
