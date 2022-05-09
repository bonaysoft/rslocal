FROM rust:1.59 as builder
WORKDIR /opt/rslocal
COPY . .
RUN apt-get update && apt-get install -y cmake protobuf-compiler && rm -rf /var/lib/apt/lists/*
RUN cargo install --path .

FROM debian:buster-slim
COPY rslocald.toml /etc/rslocal/rslocald.toml
COPY --from=builder /usr/local/cargo/bin/rslocald /usr/local/bin/rslocald
CMD ["rslocald"]