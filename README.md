# seedcollection
A basic tool for tracking seed collections. It contains a commandline client and
a web client. It is not expected to work for you. It barely works for me.

This software is provided under the terms of either the [Apache 2.0 license](LICENSE-APACHE) or the [MIT License](LICENSE-MIT)

## Building the application
To build the rust code, simply run `cargo build`

## Preparing the database
To prepare the database, run `make prepare-db`. This will download the latest
ITIS database from the internet, customize it slightly, and then prompt you
for a username/email/password to create your first user account. The prepared
database will be located in `./db/itis/seedcollection.sqlite`. This database can
then be used with `seedctl` or `seedweb`.

## Running the application
In order to run the web application, several javascript packages need to be
installed. To do this, do the following:

```
  $ cd web/vendor-js
  $ yarn
```

In order to run the web application in a docker or podman container, simply run
`make run-container`
