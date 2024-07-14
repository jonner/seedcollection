container: Dockerfile config.yaml.docker
	podman build -t seedweb:latest .

run-container: container
	podman run --detach \
	--tty \
	--secret seedweb-smtp-password \
	-p 8080:80 -p 8443:443 \
	-v ~/.local/share/seedcollection/:/usr/share/seedweb/db:Z \
	seedweb:latest
