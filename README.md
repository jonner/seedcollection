# seedcollection
A basic tool for tracking seed collections. It contains a commandline client and
a web client. It is not expected to work for you. It barely works for me.

This software is provided under the terms of either the [Apache 2.0 license](LICENSE-APACHE) or the [MIT License](LICENSE-MIT)

## Building the application
To build the rust code, simply run `cargo build`

The web client needs some additional javascript packages to function. To install those locally, run `yarn install`

## Running the application
In order to run the web application in a docker or podman container, simply run `make run-container`
