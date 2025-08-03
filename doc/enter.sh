#!/bin/sh
docker build --network=host -t riff-doc .
docker run --rm -it -e THEUID="$(id -u "$USER")" -v "$PWD":/var/doxerlive riff-doc ash
