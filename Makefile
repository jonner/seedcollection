# Choose between podman and docker, preferring podman
ifeq ($(shell command -v podman 2> /dev/null),)
    CONTAINERCMD=docker
else
    CONTAINERCMD=podman
endif

SEEDWEB_DATABASE_DIR ?= ./db/itis

SEEDWEB_HTTP_PORT ?= 8080
SEEDWEB_HTTPS_PORT ?= 8443
SEEDWEB_LOG ?= debug

update-container: Containerfile
	$(CONTAINERCMD) pull rust:alpine alpine:latest

container: Containerfile config.yaml.docker
	$(CONTAINERCMD) build -t seedweb:latest .

RUN_CONTAINER=$(CONTAINERCMD) run --detach \
	--name seedweb \
	--replace \
	--tty \
	--secret seedweb-smtp-password,type=env,target=SEEDWEB_SMTP_PASSWORD \
	--env SEEDWEB_LOG=${SEEDWEB_LOG} \
	-p ${SEEDWEB_HTTP_PORT}:80 -p ${SEEDWEB_HTTPS_PORT}:443 \
	-v ${SEEDWEB_DATABASE_DIR}:/usr/share/seedweb/db:Z \
	seedweb:latest $(SEEDWEB_CMD)

run-container: container
	@if [ -e ${SEEDWEB_DATABASE_DIR}/seedcollection.sqlite ]; then \
		$(RUN_CONTAINER); \
	else \
		echo "Database not found. You can run 'make prepare-db' to generate a database for use with this software"; \
		exit 1; \
	fi

run-dev-container: container
	@if [ -e ${SEEDWEB_DATABASE_DIR}/seedcollection.sqlite ]; then \
		$(RUN_CONTAINER) --env dev; \
	else \
		echo "Database not found. You can run 'make prepare-db' to generate a database for use with this software"; \
		exit 1; \
	fi

# run this to update the cached sql queries for offline building. DATABASE_URL
# must be set to the url of a valid database with the correct schema
sqlx-prepare: check-sqlx-env
	cargo sqlx prepare --workspace -- --all-targets

check-sqlx-env:
ifndef DATABASE_URL
	$(error Set DATABASE_URL to the location of the database before running)
endif


##################
# DATABASE SETUP #
##################
INIT_DB ?= $(SEEDWEB_DATABASE_DIR)/seedcollection.sqlite
INIT_DB_ARGS ?= --download
prepare-db: ./db/itis/minnesota-native-status.csv ./db/germination/germination-data.csv
	cargo run -p seedctl -- admin -d $(INIT_DB) database init $(INIT_DB_ARGS)
	cargo run -p seedctl -- admin -d $(INIT_DB) database update-native-status ./db/itis/minnesota-native-status.csv
	cargo run -p seedctl -- admin -d $(INIT_DB) database update-germination-info ./db/germination/germination-data.csv

.PHONY: check-sqlx-env prepare-db container run-container
