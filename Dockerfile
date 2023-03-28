FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release

# We do not need the Rust toolchain to run the binary!
FROM debian:buster AS runtime
WORKDIR app
RUN apt-get update -y && \
    apt-get install libssl1.1 && \
    dpkg -L libssl1.1 && \
    apt update -y
COPY --from=builder /app/application.yml /app/application.yml
COPY --from=builder /app/target/release/search-universe-listener /usr/local/bin/app
EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/app"]
