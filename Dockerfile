# syntax=docker/dockerfile:1
FROM rust:1.99.1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/main.rs ./src/main.rs
ARG CARGO_NET_OFFLINE=false
RUN --mount=type=cache,id=rustpress-registry,target=/usr/local/cargo/registry,sharing=locked \
    cargo fetch --locked
COPY src ./src
COPY migrations ./migrations
COPY docs/agents.txt ./docs/agents.txt
COPY openapi.json ./
ARG CARGO_PROFILE=release
RUN --mount=type=cache,id=rustpress-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=rustpress-target-${CARGO_PROFILE},target=/build/target,sharing=locked \
    cargo build --profile "$CARGO_PROFILE" --locked --offline \
    && cp "target/$CARGO_PROFILE/rustpress" /build/rustpress

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
RUN groupadd --gid 10001 rustpress && useradd --uid 10001 --gid rustpress --no-create-home rustpress \
    && mkdir -p /data/media && chown -R rustpress:rustpress /data
COPY --from=build /build/rustpress /usr/local/bin/rustpress
USER 10001:10001
ENV BIND_ADDR=0.0.0.0:8080 MEDIA_DIR=/data/media
EXPOSE 8080
ENTRYPOINT ["rustpress"]
