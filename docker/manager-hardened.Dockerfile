ARG BASE_IMAGE

FROM ${BASE_IMAGE}

USER root
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

ENV HOME=/home/teamhub
USER 10001:10001
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=10s --start-period=120s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8080/api/health >/dev/null && curl -fsS http://127.0.0.1:9800/health >/dev/null || exit 1

CMD ["/usr/bin/supervisord", "-n", "-c", "/etc/supervisor/conf.d/ts6.conf"]
