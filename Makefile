# Choose between podman and docker, preferring podman
ifeq ($(shell command -v podman 2> /dev/null),)
    CONTAINERCMD=docker
else
    CONTAINERCMD=podman
endif

SEEDWEB_DATABASE_DIR ?= ./db/itis

SEEDWEB_HTTP_PORT ?= 8080
SEEDWEB_HTTPS_PORT ?= 8443

container: Dockerfile config.yaml.docker
	$(CONTAINERCMD) build -t seedweb:latest .

run-container: container
	@if [ -e ${SEEDWEB_DATABASE_DIR}/seedcollection.sqlite ]; then \
		$(CONTAINERCMD) run --detach \
		--name seedweb \
		--replace \
		--tty \
		--secret seedweb-smtp-password,type=env,target=SEEDWEB_SMTP_PASSWORD \
		-p ${SEEDWEB_HTTP_PORT}:80 -p ${SEEDWEB_HTTPS_PORT}:443 \
		-v ${SEEDWEB_DATABASE_DIR}:/usr/share/seedweb/db:Z \
		seedweb:latest; \
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
./db/itis/itisSqlite.zip:
	curl https://www.itis.gov/downloads/itisSqlite.zip --output $@
CLEANDBFILES+=./db/itis/itisSqlite.zip

./db/itis/seedcollection.sqlite.orig: ./db/itis/itisSqlite.zip
	unzip -j $< -d ./db/itis/ "*/ITIS.sqlite"
	mv ./db/itis/ITIS.sqlite $@
	touch $@
CLEANDBFILES+=./db/itis/seedcollection.sqlite.orig

./db/itis/seedcollection.sqlite.stamp: ./db/itis/seedcollection.sqlite.orig ./db/itis/minnesota-itis-input-modified.csv
	python ./db/itis/match-species.py --updatedb -d $^
	cargo run -p seedctl -- init -d $<
	touch $@
CLEANDBFILES+=./db/itis/ITIS.sqlite.stamp

./db/itis/seedcollection.sqlite: ./db/itis/seedcollection.sqlite.stamp
	cp ./db/itis/seedcollection.sqlite.orig $@
CLEANDBFILES+=./db/itis/seedcollection.sqlite

prepare-db: ./db/itis/seedcollection.sqlite

clean-db:
	@rm -f $(CLEANDBFILES)

.PHONY: check-sqlx-env prepare-db clean-db container run-container
