# ── Build stage ─────────────────────────────────────────────────────────────
FROM rust:alpine AS builder

# musl-dev provides the C headers & linker needed for musl targets
RUN apk add --no-cache musl-dev

WORKDIR /build

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

# ── Runtime stage ────────────────────────────────────────────────────────────
FROM alpine:3.21

RUN apk add --no-cache ca-certificates tzdata

COPY --from=builder /build/target/release/haruki-hmes /usr/local/bin/haruki-hmes

ENV HMES_HOST=0.0.0.0 \
    HMES_PORT=7910

EXPOSE 7910

ENTRYPOINT ["/usr/local/bin/haruki-hmes"]
