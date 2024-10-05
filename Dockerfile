FROM rust:alpine as builder
WORKDIR /usr/local/seedcollection
RUN --mount=type=cache,target=/var/cache/apk \
  apk add \
  musl-dev \
  openssl-dev \
  pkgconf
COPY . .
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN \
  --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/usr/local/seedcollection/target \
  cargo install --path ./web --root /usr/local

FROM alpine:latest as runtime-base
RUN --mount=type=cache,target=/var/cache/apk \
  apk add \
  ca-certificates \
  libgcc \
  openssl

FROM runtime-base
COPY ./config.yaml.docker /etc/seedweb/config.yaml
COPY ./certs /etc/seedweb/certs
COPY ./web/static /usr/share/seedweb/static/
COPY ./node_modules /usr/share/seedweb/static/js
COPY ./web/templates /usr/share/seedweb/templates/
VOLUME /usr/share/seedweb/db
COPY --from=builder /usr/local/bin/seedweb /usr/local/bin
EXPOSE 80
EXPOSE 443
ENV SEEDWEB_LOG=debug
ENTRYPOINT ["seedweb"]
CMD ["--env", "prod"]
