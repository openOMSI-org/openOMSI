# syntax=docker/dockerfile:1.7
# The openOMSI dedicated server on Linux: the game binary started with --server, with no
# window, no sound and no graphics card (wgpu's no-op device). Build from the repository root:
#   docker build -f docker/server/Dockerfile -t openomsi-server .
# or with docker compose (docker/server/docker-compose.yml). It needs an OMSI 2 installation
# mounted at /omsi (read only) to start; see docker/server/README.md.

FROM rust:1-bookworm AS build
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake pkg-config libasound2-dev libudev-dev libgtk-3-dev libxkbcommon-dev libwayland-dev libssl-dev \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# (no .git in the context: the version is given, else VERSION's number)
ARG OPENOMSI_VERSION=""
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    export OPENOMSI_VERSION="${OPENOMSI_VERSION:-$(tr -d ' \r\n' < VERSION)}" \
 && cargo build --locked --release -p omsi-app \
 && mkdir -p /out \
 && cp target/release/openomsi /out/ \
 && strip --strip-debug /out/openomsi \
 && (cp target/release/libsteam_api.so /out/ 2>/dev/null || true) \
 && ldd /out/openomsi

FROM debian:bookworm-slim
# the libraries the program is linked against (ldd: ALSA, udev and Steam's library beside
# it - the window and graphics libraries are opened only when it draws, which a server never
# does); curl for the health check
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libasound2 libudev1 curl \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/ /opt/openomsi/
ENV LD_LIBRARY_PATH=/opt/openomsi
# the players' buses (lan-store), the logs and the server's data: a volume, so they outlive
# the container
VOLUME ["/root/.openomsi"]
# UDP: the session; TCP on the same number: the mods (and players' buses); TCP 27025: the web
# gateway (status, icon, WebSocket players)
EXPOSE 27015/udp 27015/tcp 27025/tcp
HEALTHCHECK --interval=30s --timeout=5s --start-period=10m CMD curl -fsS http://127.0.0.1:27025/status >/dev/null || exit 1
ENTRYPOINT ["/opt/openomsi/openomsi"]
CMD ["--root", "/omsi", "--server", "/server/server.cfg"]
