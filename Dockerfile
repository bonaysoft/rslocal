FROM rust:1.59 as builder

RUN apt-get update && apt-get install -y cmake protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/rslocal
COPY . .
RUN cargo install --path .

FROM debian:buster-slim
COPY --from=builder /usr/local/cargo/bin/rslocald /usr/local/bin/rslocald
CMD ["rslocald"]