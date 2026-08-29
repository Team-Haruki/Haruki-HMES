# ── Build stage ─────────────────────────────────────────────────────────────
FROM rust:alpine AS builder

# musl-dev provides the C headers & linker needed for musl targets
RUN apk add --no-cache musl-dev

WORKDIR /build

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
