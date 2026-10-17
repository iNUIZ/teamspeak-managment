ARG BASE_IMAGE
FROM ${BASE_IMAGE}

USER root
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get upgrade -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
USER teamspeak:teamspeak
