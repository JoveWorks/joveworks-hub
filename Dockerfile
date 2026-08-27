FROM rust:1.94-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 joveworks \
    && mkdir --parents /var/lib/joveworks \
    && chown joveworks:joveworks /var/lib/joveworks
COPY --from=build /src/target/release/joveworks_hub /usr/local/bin/joveworks-hub
VOLUME ["/var/lib/joveworks"]
USER joveworks
ENV JOVEWORKS_BIND=0.0.0.0:8080
ENV JOVEWORKS_DATABASE_URL=sqlite:///var/lib/joveworks/hub.sqlite?mode=rwc
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/joveworks-hub"]
