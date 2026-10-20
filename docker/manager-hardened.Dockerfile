ARG BASE_IMAGE

# Isolated runtime: the Manager stays on its own Node version. Both the plugin
# and provider come from the SAME immutable, checksum-verified source archive.
FROM node:24-bookworm-slim AS pot-build
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl python3 build-essential pkg-config libcairo2-dev \
      libpango1.0-dev libjpeg-dev libgif-dev librsvg2-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/teamhub-pot
RUN curl -fsSL --proto '=https' --tlsv1.2 \
      https://codeload.github.com/Brainicism/bgutil-ytdlp-pot-provider/tar.gz/26475e9d3665b972c4aeb83b4dbaafcf64b88438 \
      -o source.tar.gz \
    && echo 'a8144eab6a4f38f320063688531f461afe18c0a58be2dd3152b780f4851e954d  source.tar.gz' | sha256sum -c - \
    && mkdir source plugins \
    && tar -xzf source.tar.gz --strip-components=1 -C source \
    && cp -a source/plugin plugins/bgutil
WORKDIR /opt/teamhub-pot/source/server
RUN npm ci --ignore-scripts --no-audit --no-fund \
    && npm rebuild canvas --build-from-source \
    && ./node_modules/.bin/tsc \
    && npm prune --omit=dev --ignore-scripts --no-audit --no-fund

FROM ${BASE_IMAGE}

USER root
COPY --from=pot-build /usr/local/bin/node /opt/teamhub-pot/node
COPY --from=pot-build /opt/teamhub-pot/source/server /opt/teamhub-pot/server
COPY --from=pot-build /opt/teamhub-pot/plugins /opt/teamhub-pot/plugins
# Corresponding source and license accompany the GPL-3.0-only component.
COPY --from=pot-build /opt/teamhub-pot/source.tar.gz /opt/teamhub-pot/source.tar.gz
COPY --from=pot-build /opt/teamhub-pot/source/LICENSE /opt/teamhub-pot/LICENSE
COPY docker/quiet-pot-provider.mjs /opt/teamhub-pot/quiet-provider.mjs
RUN apt-get update && apt-get install -y --no-install-recommends \
      libcairo2 libpango-1.0-0 libpangocairo-1.0-0 libjpeg62-turbo libgif7 librsvg2-2 \
    && rm -rf /var/lib/apt/lists/*
ARG INSTALL_TTS=false
COPY docker/install-tts.sh /tmp/install-tts.sh
RUN if [ "$INSTALL_TTS" = true ]; then sh /tmp/install-tts.sh; fi && rm -f /tmp/install-tts.sh

COPY docker/patch-manager-auth.mjs /tmp/patch-manager-auth.mjs
COPY docker/manager-nginx.conf /etc/nginx/nginx.conf
COPY docker/manager-supervisord.conf /etc/supervisor/conf.d/ts6.conf

RUN node /tmp/patch-manager-auth.mjs \
    && rm -f /tmp/patch-manager-auth.mjs \
    && apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get upgrade -y \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 teamhub \
    && useradd --uid 10001 --gid 10001 --create-home --shell /usr/sbin/nologin teamhub \
    && install -d -o 10001 -g 10001 /app/packages/backend/data /data/music \
       /home/teamhub/.config/yt-dlp /tmp/nginx-client /tmp/nginx-proxy \
       /tmp/nginx-fastcgi /tmp/nginx-uwsgi /tmp/nginx-scgi \
    && printf '%s\n' '--js-runtimes deno' > /home/teamhub/.config/yt-dlp/config \
    && chown 10001:10001 /home/teamhub/.config/yt-dlp/config \
    && chown -R 10001:10001 /app/packages/backend /data/music /var/www/html

ENV HOME=/home/teamhub \
    YOUTUBE_POT_PLUGIN_DIR=/opt/teamhub-pot/plugins \
    YOUTUBE_POT_URL=http://127.0.0.1:4416
USER 10001:10001
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=10s --start-period=120s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/api/health >/dev/null && curl -fsS http://127.0.0.1:9800/health >/dev/null || exit 1

CMD ["/usr/bin/supervisord", "-n", "-c", "/etc/supervisor/conf.d/ts6.conf"]
