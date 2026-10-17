# Dev image for the DD-L0-LA59 simulator: cross-compiles the Windows exe with
# MinGW so nothing is installed on the host (same approach as parchment-dev).
FROM rust:1.92-bookworm

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        gcc-mingw-w64-x86-64-posix g++-mingw-w64-x86-64-posix binutils-mingw-w64-x86-64 \
        unzip ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*

RUN rustup target add x86_64-pc-windows-gnu \
 && rustup component add rustfmt clippy

# The official Windows embeddable Python, staged beside the exe at build time
# so the simulator needs no Python on the host. Checksum pinned.
ARG PY_VERSION=3.13.5
ARG PY_SHA256=7d2650fd9d1b9d002d4a315d5f354247fd6a44f30517c7ef577b08f57a0fb6d9
RUN mkdir -p /opt/python-embed \
 && curl -sSfL -o /tmp/py.zip "https://www.python.org/ftp/python/${PY_VERSION}/python-${PY_VERSION}-embed-amd64.zip" \
 && echo "${PY_SHA256}  /tmp/py.zip" | sha256sum -c - \
 && unzip -q /tmp/py.zip -d /opt/python-embed \
 && rm /tmp/py.zip

WORKDIR /workspace
