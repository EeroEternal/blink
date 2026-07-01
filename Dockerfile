# syntax=docker/dockerfile:1.7

FROM rust:bookworm AS builder
ARG DEBIAN_FRONTEND=noninteractive
ARG BOXLITE_RUNTIME_URL=https://github.com/boxlite-ai/boxlite/releases/download/v0.9.5/boxlite-runtime-v0.9.5-linux-x64-gnu.tar.gz

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        curl \
        libssl-dev \
        pkg-config \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

ENV BOXLITE_DEPS_STUB=2 \
    BOXLITE_RUNTIME_URL=${BOXLITE_RUNTIME_URL}

RUN cargo build --release --locked -p blink-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/blink-server /usr/local/bin/blink-server

EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/blink-server"]
CMD ["--bind", "0.0.0.0", "--port", "8787"]
