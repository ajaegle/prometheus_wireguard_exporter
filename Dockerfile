# BUILDER
FROM rust:1.35 as build

# prepare for musl
RUN apt-get update
RUN apt-get install musl-tools -y
RUN rustup target add x86_64-unknown-linux-musl

# create a new empty shell project
RUN USER=root cargo new --bin prometheus_wireguard_exporter
WORKDIR /prometheus_wireguard_exporter

COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml

# cache dependencies
RUN RUSTFLAGS=-Clinker=musl-gcc cargo build --release --target x86_64-unknown-linux-musl
RUN rm src/*.rs

COPY ./src ./src 

RUN rm ./target/x86_64-unknown-linux-musl/release/deps/prometheus_wireguard_exporter*
RUN RUSTFLAGS=-Clinker=musl-gcc cargo build --release --target x86_64-unknown-linux-musl


# RESULTING IMAGE
# leveraging jessfraz prebuilt wg image
FROM r.j3ss.co/wg:tools

COPY --from=build /prometheus_wireguard_exporter/target/x86_64-unknown-linux-musl/release/prometheus_wireguard_exporter .
ENTRYPOINT ["./prometheus_wireguard_exporter"]
