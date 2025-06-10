SEEDWEB_DATABASE_DIR ?= ./db/itis
SEEDWEB_ENV ?= dev
SEEDWEB_HTTP_PORT ?= 8080
SEEDWEB_LOG ?= debug
SEEDWEB_SMTP_PASSWORD_FILE ?= ~/.config/seedcollection.smtp.pwd

export SEEDWEB_DATABASE_DIR
export SEEDWEB_ENV
export SEEDWEB_HTTP_PORT
export SEEDWEB_LOG

pull-container: Containerfile
	podman pull rust:alpine alpine:latest

update-nodejs: web/vendor-js/package.json
	cd web/vendor-js && yarn

container: Containerfile update-nodejs
	podman build -t seedweb:latest .

run-pod: container deploy/seedweb-pod.yaml
	export SEEDWEB_SMTP_PASSWORD=$$(cat $(SEEDWEB_SMTP_PASSWORD_FILE)| base64);\
	cat ./deploy/seedweb-pod.yaml | envsubst | podman kube play --replace -

stop-pod: 
	cat ./deploy/seedweb-pod.yaml | envsubst | podman kube down -

# run this to update the cached sql queries for offline building. DATABASE_URL
# must be set to the url of a valid database with the correct schema
sqlx-prepare: check-sqlx-env
	cargo sqlx prepare --workspace -- --all-targets

check-sqlx-env:
ifndef DATABASE_URL
	$(error Set DATABASE_URL to the location of the database before running)
endif

generate-completions:
	cargo run -p seedctl -- generate-completions bash > ~/.local/share/bash-completion/completions/seedctl


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
