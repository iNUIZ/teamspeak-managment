ARG BASE_IMAGE
FROM ${BASE_IMAGE}

USER root
RUN apk upgrade --no-cache
USER ts3server
