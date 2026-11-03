# Dev image for the DD-L0-LA59 simulator: cross-compiles the Windows exe with
# MinGW and builds the Linux one natively, so nothing is installed on the host
# (same approach as parchment-dev).
FROM rust:1.92-bookworm

# The -dev packages are for the Linux build: ALSA (audio), udev, and the
# Wayland and keyboard libraries winit builds against.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        gcc-mingw-w64-x86-64-posix g++-mingw-w64-x86-64-posix binutils-mingw-w64-x86-64 \
        unzip ca-certificates curl python3 \
        pkg-config libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev \
 && rm -rf /var/lib/apt/lists/*

# python3 runs tests/headless.py (the scenario checks) at the end of build.sh.

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

# The Linux build links against, and bundles, a relocatable CPython of the
# same version from python-build-standalone (python.org has no relocatable
# Linux build). Checksum pinned from the release's SHA256SUMS.
ARG PBS_TAG=20250708
ARG PBS_SHA256=5b16ef64075d941933acf4e4ada7b0c7d5925ce5a2e053e905b5c148ada1bdfe
RUN curl -sSfL -o /tmp/py.tar.gz "https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_TAG}/cpython-${PY_VERSION}+${PBS_TAG}-x86_64-unknown-linux-gnu-install_only.tar.gz" \
 && echo "${PBS_SHA256}  /tmp/py.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/py.tar.gz -C /opt \
 && mv /opt/python /opt/python-linux \
 && rm /tmp/py.tar.gz

WORKDIR /workspace
