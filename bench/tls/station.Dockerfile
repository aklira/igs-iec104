# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
#
# The station under test with TLS (task X1): igs104-server, and the command-line client igs104-client, built in
# the container with the OpenSSL backend (the default feature), run from a Debian image with libssl3. The
# entrypoint is the station; the client is run with an entrypoint override. Built from the repository root:
#
#   podman build --file bench/tls/station.Dockerfile .
#
# Both stages are pinned by digest. The station runs as a non-root user with a fixed identity (10001).

ARG RUST_IMAGE=docker.io/library/rust@sha256:6ff07edce8775d0f64be7aba9197229407301bddf2054d62c27b541a6238a181
ARG RUNTIME_IMAGE=docker.io/library/debian@sha256:918311b7b6c4c6f68b232ba516584925f6c78ad82b6fd534b98979df6438e483

FROM ${RUST_IMAGE} AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p igs-iec104 --bin igs104-server --bin igs104-client

FROM ${RUNTIME_IMAGE}
RUN apt-get update \
    && apt-get install -y --no-install-recommends libssl3t64 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 station \
    && useradd --system --uid 10001 --gid station --no-create-home --shell /usr/sbin/nologin station
COPY --from=build /src/target/release/igs104-server /usr/local/bin/igs104-server
COPY --from=build /src/target/release/igs104-client /usr/local/bin/igs104-client
USER 10001:10001
ENTRYPOINT ["/usr/local/bin/igs104-server"]
