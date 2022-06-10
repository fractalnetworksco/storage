FROM debian:11

ARG BUILD_TYPE=release
ENV STORAGE_LISTEN=0.0.0.0:8000
ENV STORAGE_DATABASE=sqlite:///tmp/gateway.db?create=true
ENV RUST_LOG=info,sqlx=warn
ENV RUST_BACKTRACE=1

COPY /target/$BUILD_TYPE/fractal-storage /usr/local/bin/fractal-storage
COPY scripts/entrypoint.sh /bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/fractal-storage"]
