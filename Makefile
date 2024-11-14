# Choose between podman and docker, preferring podman
ifeq ($(shell command -v podman 2> /dev/null),)
    CONTAINERCMD=docker
else
    CONTAINERCMD=podman
endif

SEEDWEB_HTTP_PORT ?= 8080
SEEDWEB_HTTPS_PORT ?= 8443

container: Dockerfile config.yaml.docker
	$(CONTAINERCMD) build -t seedweb:latest .

run-container: container
	$(CONTAINERCMD) run --detach \
	--name seedweb \
	--replace \
	--tty \
	--secret seedweb-smtp-password,type=env,target=SEEDWEB_SMTP_PASSWORD \
	-p ${SEEDWEB_HTTP_PORT}:80 -p ${SEEDWEB_HTTPS_PORT}:443 \
	-v ~/.local/share/seedcollection/:/usr/share/seedweb/db:Z \
	seedweb:latest

# run this to update the cached sql queries for offline building. DATABASE_URL
# must be set to the url of a valid database with the correct schema
sqlx-prepare: check-sqlx-env
	cargo sqlx prepare --workspace -- --all-targets

check-sqlx-env:
ifndef DATABASE_URL
	$(error Set DATABASE_URL to the location of the database before running)
endif

.PHONY: check-sqlx-env
