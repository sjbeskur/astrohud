FROM debian:bookworm-slim

ARG IMAGE_GEN_COMMIT=3f2c916086ad70197945bfc50ef953c1f6035f10
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git qemu-user-static binfmt-support arch-test

RUN git clone --depth 1 --branch v2.6.0 https://github.com/raspberrypi/rpi-image-gen.git /opt/rpi-image-gen \
    && test "$(git -C /opt/rpi-image-gen rev-parse HEAD)" = "$IMAGE_GEN_COMMIT" \
    && awk -F: 'NF && $0 !~ /^[[:space:]]*#/ {print ($3 != "" ? $3 : $2)}' /opt/rpi-image-gen/depends | sort -u > /tmp/image-dependencies \
    && xargs apt-get install -y --no-install-recommends < /tmp/image-dependencies \
    && rm -rf /var/lib/apt/lists/*
ENV RPI_IMAGE_GEN_DIR=/opt/rpi-image-gen
WORKDIR /workspace
ENTRYPOINT ["bash", "/workspace/astrohud-image/scripts/container-entry.sh"]
