FROM rust:1-bookworm AS builder
WORKDIR /src
COPY . .
RUN cargo build --release -p sdkwork-api-merchandise-standalone-gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /src/target/release/sdkwork-api-merchandise-standalone-gateway /usr/local/bin/sdkwork-api-merchandise-standalone-gateway
ENV SDKWORK_MERCHANDISE_APPLICATION_PUBLIC_INGRESS_BIND=0.0.0.0:18090
EXPOSE 18090
CMD ["sdkwork-api-merchandise-standalone-gateway"]
