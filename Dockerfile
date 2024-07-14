FROM rust:alpine as builder
WORKDIR /usr/local/seedcollection
COPY . .
RUN apk --no-cache add \
  openssl-dev musl-dev pkgconf

FROM builder as build
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo install --path ./web --root /usr/local

FROM alpine:latest as runtime-base
RUN apk --no-cache add libgcc openssl ca-certificates

FROM runtime-base
COPY --from=build /usr/local/bin/seedweb /usr/local/bin
COPY ./config.yaml.docker /etc/seedweb/config.yaml
COPY ./certs /etc/seedweb/certs
COPY ./web/static /usr/share/seedweb/static/
COPY ./web/templates /usr/share/seedweb/templates/
EXPOSE 80
EXPOSE 443
ENV SEEDWEB_LOG=debug
CMD ["seedweb", "--env", "prod"]
