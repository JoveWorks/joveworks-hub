FROM rust:1.94-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 joveworks \
    && mkdir --parents /var/lib/joveworks \
    && chown joveworks:joveworks /var/lib/joveworks
COPY --from=build /src/target/release/joveworks_hub /usr/local/bin/joveworks-hub
VOLUME ["/var/lib/joveworks"]
USER joveworks
ENV JOVEWORKS_BIND=0.0.0.0:8080
ENV JOVEWORKS_DATABASE_URL=sqlite:///var/lib/joveworks/hub.sqlite?mode=rwc
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl --fail --silent --output /dev/null http://127.0.0.1:8080/healthz || exit 1
ENTRYPOINT ["/usr/local/bin/joveworks-hub"]
