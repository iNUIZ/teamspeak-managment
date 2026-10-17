ARG BASE_IMAGE=ubuntu:24.04@sha256:68ee22004a9109b44973313b02ff7b1de250f1de0f5ecb628d73f579aaff33ac
FROM ${BASE_IMAGE}

ARG TEASPEAK_VERSION=1.5.6
ARG TEASPEAK_SHA256=50ddd39e59c075137371ccde4c26c5930b33cc3a326230bd93d442efc57b7c9c

RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends ca-certificates curl ffmpeg openssl tzdata \
 && mkdir -p /ts/logs /ts/certs /ts/files /ts/database /ts/config /ts/crash_dumps \
 && curl -fsSL "https://repo.teaspeak.de/server/linux/amd64_stable/TeaSpeak-${TEASPEAK_VERSION}.tar.gz" -o /tmp/teaspeak.tar.gz \
 && echo "${TEASPEAK_SHA256}  /tmp/teaspeak.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/teaspeak.tar.gz -C /ts \
 && rm -f /tmp/teaspeak.tar.gz \
 && rm -f /ts/certs/* \
 && : > /ts/config/config.yml \
 && ln -sf /ts/config/config.yml /ts/config.yml \
 && groupadd --gid 4242 teaspeak \
 && useradd --uid 4242 --gid 4242 --no-create-home --home-dir /ts --shell /usr/sbin/nologin teaspeak \
 && chown -R 4242:4242 /ts \
 && apt-get purge -y --auto-remove curl \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /ts
COPY --chmod=0555 teaspeak-entrypoint.sh /usr/local/bin/teaspeak-entrypoint
ENV LD_LIBRARY_PATH=/ts/libs \
    LD_PRELOAD=/ts/libs/libjemalloc.so.2
USER 4242:4242
EXPOSE 9987/tcp 9987/udp 10101/tcp 30303/tcp
VOLUME ["/ts/logs", "/ts/certs", "/ts/config", "/ts/files", "/ts/database", "/ts/crash_dumps"]
ENTRYPOINT ["/usr/local/bin/teaspeak-entrypoint"]
CMD ["-Pgeneral.database.url=sqlite://database/TeaData.sqlite"]
