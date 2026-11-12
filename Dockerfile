FROM rust:1.85-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

RUN cargo build --locked --release --bin shopcore

FROM debian:bookworm-slim AS runtime

ENV TZ=Asia/Jakarta \
    RUST_LOG=info \
    PORT=8000 \
    EXPORT_DIR=/tmp/exports

RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates tzdata wget \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 shopcore \
    && mkdir -p /tmp/exports \
    && chown -R shopcore:shopcore /tmp/exports

COPY --from=builder /build/target/release/shopcore /usr/local/bin/shopcore

USER shopcore
EXPOSE 8000
ENTRYPOINT ["shopcore"]
CMD ["serve"]
