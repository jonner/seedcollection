# seedcollection
A basic tool for tracking seed collections. It contains a commandline client and
a web client. It is not expected to work for you. It barely works for me.

This software is provided under the terms of either the [Apache 2.0
license](LICENSE-APACHE) or the [MIT License](LICENSE-MIT)

## Components

The project consists of three components
 - A shared library that is used by the other two components: `libseed`
 - A command line client for interacting with the database: `seedctl`
 - A web application for managing the collection: `seedweb`

## Preparing the database
To prepare the database, run `make prepare-db`. This will download the latest
ITIS database from the internet, customize it slightly, and then prompt you
for a username/email/password to create your first user account. The prepared
database will be located in `./db/itis/seedcollection.sqlite`. This database can
then be used with `seedctl` or `seedweb`.

## Building the application
To build the rust code, simply run `cargo build`

## Running the application
As it's still early in the application's development, there are no neatly
packaged ways to install and run the program. For now you'll need to be a
developer or particularly adventurous to try it out.

### The Command line client
The `seedctl` binary can be run with from the working directory via `cargo run
-p seedctl`. Alternatively, you can install it to somewhere in your path (via
`cargo install`) and run it with the binary name `seedctl`.

### The Web Application
In order to run the web application, several javascript packages need to be
installed. If you want to run the web application directly from the command
line (via `cargo run -p seedweb`), you'll have to download them by doing the
following before you launch the executable:

```
  $ cd web/vendor-js
  $ yarn
```

You'll also have to move static resources (templates, js/css files, etc)
into expected locations in order to run the web application directly from the
commandline. Therefore, it is highly recommended to run the application in a
container instead. The process of building the container will automatically
download the nodejs packages for you and copy the static resources into the
appropriate locations.

You can build the container with `make container`. The container expects a
configuration file to be mounted at path `/etc/seedweb/config.yaml`. This
configuration file will the location of the database to use, which will need to
be mounted into the container at that location as a writable volume.

A complete basic development environment can also be launched with `make
run-pod`. This is not intended for serving the application publicly, it is only
intended for local development. This will start several containers within a
single pod:
 - a container running the `seedweb` web application
 - a container running the [caddy](https://caddyserver.com/) reverse proxy which
   handles SSL automatically
 - a couple containers running monitoring / metrics applications

See the Makefile for environment variables that can be used to customize the
development environment.
