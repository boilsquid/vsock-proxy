FROM rust:1.74-slim-bookworm

ARG APP_NAME=vsock-proxy
WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY . .

RUN cargo build

CMD ["/app/target/release/vsock-proxy"]
