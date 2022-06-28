FROM debian:11

# needs certificates in order to do TLS
RUN apt update && apt install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# build arg -- which binary to use
ARG BUILD_TYPE=release

# default parameters
ENV STORAGE_LISTEN=0.0.0.0:8000
ENV STORAGE_DATABASE=sqlite:///tmp/gateway.db?mode=rwc

# turn on useful logs
ENV RUST_LOG=info,sqlx=warn

COPY /target/$BUILD_TYPE/fractal-storage /usr/local/bin/fractal-storage
COPY scripts/entrypoint.sh /bin/entrypoint.sh

ENTRYPOINT ["/usr/local/bin/fractal-storage"]
