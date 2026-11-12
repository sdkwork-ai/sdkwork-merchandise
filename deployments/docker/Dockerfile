FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p sdkwork-api-merchandise-standalone-gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /src/target/release/sdkwork-api-merchandise-standalone-gateway /usr/local/bin/sdkwork-api-merchandise-standalone-gateway
# The database module (manifest, baseline DDL, migrations, seeds) must ship with
# the image: `bootstrap_merchandise_database` loads it through
# `SDKWORK_MERCHANDISE_APP_ROOT` at boot to run init -> migrate -> seed.
COPY --from=builder /src/database /app/database
ENV SDKWORK_MERCHANDISE_APP_ROOT=/app
ENV SDKWORK_MERCHANDISE_APPLICATION_PUBLIC_INGRESS_BIND=0.0.0.0:18090
EXPOSE 18090
CMD ["sdkwork-api-merchandise-standalone-gateway"]
