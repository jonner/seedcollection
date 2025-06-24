FROM rust:alpine AS builder
WORKDIR /usr/local/seedcollection
RUN --mount=type=cache,target=/var/cache/apk \
  apk add \
  musl-dev \
  openssl-dev \
  pkgconf \
  yarn
COPY --exclude=target --exclude=db/itis . .
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/seedcollection/target \
  cargo install --path ./web --root /usr/local
WORKDIR web/vendor-js
RUN yarn

FROM alpine:latest AS runtime-base
RUN --mount=type=cache,target=/var/cache/apk \
  apk add \
  ca-certificates \
  libgcc \
  openssl

FROM runtime-base
WORKDIR /usr/share/seedweb
COPY ./web/static static/
COPY ./web/templates templates/
VOLUME /usr/share/seedweb/db
COPY --from=builder /usr/local/seedcollection/web/vendor-js/node_modules static/js/vendor
COPY --from=builder /usr/local/bin/seedweb /usr/local/bin
EXPOSE 80
EXPOSE 443
ENV SEEDWEB_LOG=debug
ENTRYPOINT ["seedweb"]
CMD ["--env", "prod"]
