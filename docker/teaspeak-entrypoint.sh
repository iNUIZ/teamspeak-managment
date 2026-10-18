#!/bin/sh
set -eu

umask 077
mkdir -p /ts/certs

generate_certificate() {
  name=$1
  cert="/ts/certs/${name}_certificate.pem"
  key="/ts/certs/${name}_privatekey.pem"
  if [ ! -s "$cert" ] || [ ! -s "$key" ]; then
    rm -f "$cert" "$key"
    openssl req -x509 -newkey rsa:3072 -sha256 -nodes -days 825 \
      -subj "/CN=TeaSpeak ${name}" -keyout "$key" -out "$cert"
  fi
}

generate_certificate default
generate_certificate query

exec /ts/TeaSpeakServer "$@"
