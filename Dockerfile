# The ferricstore2 node (ADR-019). Build: docker build -t ferricstore2 .
FROM rust:1.99-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked --bin ferricstore2

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home /data ferricstore \
 && mkdir -p /data && chown ferricstore /data
COPY --from=build /src/target/release/ferricstore2 /usr/local/bin/ferricstore2
USER ferricstore
# Every setting is a FERRICSTORE_* variable; see deploy/README.md.
ENV FERRICSTORE_DATA_DIR=/data \
    FERRICSTORE_BIND_HOST=0.0.0.0 \
    FERRICSTORE_NATIVE_PORT=6388 \
    FERRICSTORE_PEER_PORT=7388 \
    FERRICSTORE_HEALTH_PORT=6380
EXPOSE 6388 7388 6380
VOLUME ["/data"]
ENTRYPOINT ["/usr/local/bin/ferricstore2"]
