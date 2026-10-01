# ── Build stages (cargo-chef: dependencies are cached in their own layer) ────
FROM lukemathwalker/cargo-chef:0.1.78-rust-alpine AS chef

# musl-dev provides the C headers & linker needed for musl targets
RUN apk add --no-cache musl-dev

WORKDIR /build

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

# ── Runtime stage ────────────────────────────────────────────────────────────
FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S haruki \
    && adduser -S -D -H -G haruki haruki

COPY --from=builder /build/target/release/haruki-hmes /usr/local/bin/haruki-hmes

ENV HMES_HOST=0.0.0.0 \
    HMES_PORT=7910

EXPOSE 7910

USER haruki

ENTRYPOINT ["/usr/local/bin/haruki-hmes"]
