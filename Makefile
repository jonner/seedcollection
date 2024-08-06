container: Dockerfile config.yaml.docker
	podman build -t seedweb:latest .

run-container: container
	podman run --detach \
	--name seedweb \
	--replace \
	--tty \
	--secret seedweb-smtp-password \
	-p 8080:80 -p 8443:443 \
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
