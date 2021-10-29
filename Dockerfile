FROM ubuntu:20.04

ENV STORAGE_PORT=8000
ENV STORAGE_DATABASE=/tmp/gateway.db
ENV STORAGE_PATH=/var/tmp/storage

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=${STORAGE_PORT}
ENV RUST_LOG=info,sqlx=warn
ENV RUST_BACKTRACE=1

COPY /target/release/storage /usr/local/bin/storage
RUN echo "#!/bin/bash\nmkdir -p $STORAGE_PATH\ntouch $STORAGE_DATABASE\nstorage --database \$STORAGE_DATABASE --storage $STORAGE_PATH" > /bin/start.sh
RUN chmod +x /bin/start.sh

ENTRYPOINT ["/bin/start.sh"]
