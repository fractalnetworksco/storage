FROM debian:11

ENV STORAGE_PORT=8000
ENV STORAGE_DATABASE=/tmp/gateway.db
ENV STORAGE_PATH=/var/tmp/storage

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=${STORAGE_PORT}
ENV RUST_LOG=info,sqlx=warn
ENV RUST_BACKTRACE=1

COPY /target/release/fractal-storage /usr/local/bin/fractal-storage
COPY scripts/entrypoint.sh /bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/fractal-storage"]
