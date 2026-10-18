#!/usr/bin/env bash
set -Eeuo pipefail

readonly APP_NAME="Teamspeak Managment"
readonly APP_VERSION="0.3.8-alpha.1"
KERNEL_NAME="$(uname -s)"
readonly KERNEL_NAME
case "$KERNEL_NAME" in
  MINGW*|MSYS*|CYGWIN*) readonly OS_FAMILY="windows"; readonly DEFAULT_INSTALL_ROOT="${HOME}/.teamhub" ;;
  *)                    readonly OS_FAMILY="linux";   readonly DEFAULT_INSTALL_ROOT="/opt/teamhub" ;;
esac
readonly INSTALL_ROOT="${TEAMHUB_ROOT:-$DEFAULT_INSTALL_ROOT}"
readonly PROJECT_NAME="${TEAMHUB_PROJECT:-teamhub}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly SCRIPT_DIR
readonly COMPOSE_FILE="${INSTALL_ROOT}/compose.yml"
readonly ENV_FILE="${INSTALL_ROOT}/.env"
readonly DEFAULT_TS3_IMAGE="teamspeak:3.13.8@sha256:6dfdfb22869adf50e1b66d024b360b786d57a85ea09e8e8fdb6cca23949b2813"
readonly DEFAULT_TS6_IMAGE="teamspeaksystems/teamspeak6-server:6.0.0-beta13.1@sha256:d845f803aecf842c2df034c959c23ff2462357bacafb3807500885c5ec170fa1"
readonly HARDENED_TEASPEAK_IMAGE="teamspeak-managment/teaspeak-hardened:1.5.6"
readonly DEFAULT_MANAGER_IMAGE="ghcr.io/uniskela/ts6-manager/all-in-one:1.11.1@sha256:f5323c743865a157293eb9028548a68941b6e8743a97597825e19fab339fc83d"
readonly HARDENED_MANAGER_IMAGE="teamspeak-managment/manager-hardened:0.3.8-alpha.1"
readonly DEFAULT_MANAGER_SOURCE_REF="789a40d0eb2e66ae419ceb01d01d7243fc7b3072"
readonly DEFAULT_MANAGER_SOURCE_SHA256="1dd4841ee2b70c2256b52f8cb431dddc097901e15f0ae2e69ec853e243a45ed5"
readonly ALPINE_IMAGE="alpine:3.20@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc"

declare -A DEFAULTS=(
  [TS3_VOICE_PORT]=9987
  [TS3_QUERY_PORT]=10011
  [TS3_WEBQUERY_PORT]=10080
  [TS3_FILE_PORT]=30033
  [TS6_VOICE_PORT]=9988
  [TS6_QUERY_SSH_PORT]=10022
  [TS6_QUERY_HTTP_PORT]=10081
  [TS6_FILE_PORT]=30034
  [TEASPEAK_VOICE_PORT]=9990
  [TEASPEAK_QUERY_PORT]=10101
  [TEASPEAK_FILE_PORT]=30303
  [MANAGER_PORT]=3000
)

info()  { printf '\033[1;34m[bilgi]\033[0m %s\n' "$*"; }
ok()    { printf '\033[1;32m[tamam]\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m[uyarı]\033[0m %s\n' "$*" >&2; }
die()   { printf '\033[1;31m[hata]\033[0m %s\n' "$*" >&2; exit 1; }

on_error() {
  local rc=$?
  printf '\033[1;31m[hata]\033[0m Satır %s çalıştırılamadı (kod: %s).\n' "${BASH_LINENO[0]}" "$rc" >&2
  exit "$rc"
}
trap on_error ERR

require_root() {
  [[ $OS_FAMILY == windows ]] && return
  [[ ${EUID} -eq 0 ]] || die "Bu kurucu root yetkisi ister: sudo bash install.sh"
}

check_os() {
  if [[ $OS_FAMILY == windows ]]; then
    command -v docker >/dev/null 2>&1 || die "Windows'ta önce Docker Desktop kurup başlatın."
    docker compose version >/dev/null 2>&1 || die "Docker Compose eklentisi bulunamadı."
    return
  fi
  [[ -r /etc/os-release ]] || die "Linux dağıtımı belirlenemedi."
  # shellcheck disable=SC1091
  source /etc/os-release
  case "${ID:-}" in
    ubuntu|debian) ;;
    *) die "Şimdilik yalnız Debian ve Ubuntu destekleniyor (bulunan: ${ID:-bilinmiyor})." ;;
  esac
  case "$(uname -m)" in
    x86_64|aarch64|arm64) ;;
    *) die "Desteklenmeyen işlemci mimarisi: $(uname -m)" ;;
  esac
}

confirm() {
  local prompt=$1 default=${2:-N} answer
  local hint="[e/H]"
  [[ $default == E ]] && hint="[E/h]"
  read -r -p "$prompt $hint " answer
  answer=${answer:-$default}
  [[ $answer =~ ^[EeYy]$ ]]
}

prompt_value() {
  local label=$1 default=$2 result
  read -r -p "$label [$default]: " result
  printf '%s' "${result:-$default}"
}

valid_port() {
  [[ $1 =~ ^[0-9]+$ ]] && (( 1 <= 10#$1 && 10#$1 <= 65535 ))
}

valid_public_host() {
  [[ $1 =~ ^[A-Za-z0-9][A-Za-z0-9.-]*$ ]]
}

port_in_use() {
  local port=$1 proto=${2:-tcp}
  if command -v ss >/dev/null 2>&1; then
    if [[ $proto == udp ]]; then
      ss -H -lun | awk '{print $5}' | grep -Eq "(^|:)${port}$"
    else
      ss -H -ltn | awk '{print $4}' | grep -Eq "(^|:)${port}$"
    fi
  else
    netstat -an 2>/dev/null | grep -Eiq "[:.]${port}[[:space:]].*(LISTENING|LISTEN)"
  fi
}

ask_port() {
  local key=$1 label=$2 proto=$3 value
  while true; do
    value=$(prompt_value "$label" "${DEFAULTS[$key]}")
    valid_port "$value" || { warn "1-65535 arasında bir port girin."; continue; }
    if port_in_use "$value" "$proto"; then
      warn "$value/$proto şu anda kullanımda. Başka bir port seçin."
      continue
    fi
    printf '%s' "$value"
    return
  done
}

random_secret() {
  command -v openssl >/dev/null 2>&1 || die "openssl bulunamadı."
  openssl rand -hex 32
}

docker_arch() {
  docker info --format '{{.Architecture}}' 2>/dev/null || uname -m
}

docker_platform() {
  case "$(docker_arch)" in
    aarch64|arm64) printf 'linux/arm64' ;;
    x86_64|amd64) printf 'linux/amd64' ;;
    *) die "Desteklenmeyen Docker mimarisi: $(docker_arch)" ;;
  esac
}

service_exists() {
  local service=$1
  docker ps -a --filter "label=com.docker.compose.project=${PROJECT_NAME}" \
    --filter "label=com.docker.compose.service=${service}" --format '{{.ID}}' | grep -q .
}

service_running() {
  local service=$1
  docker ps --filter "label=com.docker.compose.project=${PROJECT_NAME}" \
    --filter "label=com.docker.compose.service=${service}" --format '{{.ID}}' | grep -q .
}

launch_second_instance() {
  local second_root
  if [[ $OS_FAMILY == windows ]]; then
    second_root="${HOME}/.teamhub-2"
  else
    second_root="/opt/teamhub-2"
  fi
  info "İkinci bağımsız kurulum açılıyor: ${second_root} (Compose projesi: teamhub2)"
  info "Açılan menüden istediğiniz sunucu türünü seçin; kullanılan portlar reddedilecektir."
  TEAMHUB_ROOT="$second_root" TEAMHUB_PROJECT="teamhub2" bash "$0"
}

handle_existing_component() {
  local component=$1
  service_exists "$component" || return 1
  if [[ $PROJECT_NAME == teamhub ]] && confirm "${component} zaten kurulu. İkinci bağımsız sunucu kurmak istiyor musunuz?" N; then
    launch_second_instance
    return 0
  fi
  confirm "${component} zaten kurulu. Mevcut verileri koruyarak yeniden oluşturmak istiyor musunuz?" N || return 0
  return 1
}

volume_exists() {
  docker volume inspect "$1" >/dev/null 2>&1
}

detect_public_host() {
  local host
  host=$(curl -4fsS --max-time 4 https://api.ipify.org 2>/dev/null || true)
  if [[ ! $host =~ ^([0-9]{1,3}\.){3}[0-9]{1,3}$ ]]; then
    host=$(hostname -I 2>/dev/null | awk '{print $1}')
  fi
  printf '%s' "${host:-SUNUCU_IP}"
}

ensure_amd64_emulation() {
  [[ $(docker_platform) == linux/arm64 ]] || return 0
  if docker run --rm --platform linux/amd64 "$ALPINE_IMAGE" true >/dev/null 2>&1; then
    ok "amd64 emülasyonu hazır."
    return
  fi
  [[ $OS_FAMILY == linux ]] || die "Docker Desktop ayarlarında x86/amd64 emülasyonunu etkinleştirin."
  info "TS3 için QEMU/binfmt amd64 emülasyonu otomatik hazırlanıyor..."
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y qemu-user-static binfmt-support
  systemctl restart systemd-binfmt.service 2>/dev/null || true
  systemctl restart docker
  docker run --rm --platform linux/amd64 "$ALPINE_IMAGE" true >/dev/null 2>&1 \
    || die "amd64 emülasyonu otomatik etkinleştirilemedi. Çekirdekte binfmt_misc desteğini kontrol edin."
  ok "amd64 emülasyonu etkinleştirildi."
}

clamp() {
  local value=$1 minimum=$2 maximum=$3
  (( value < minimum )) && value=$minimum
  (( value > maximum )) && value=$maximum
  printf '%s' "$value"
}

detect_performance_defaults() {
  local cpu_count voice_threads
  cpu_count=$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || printf '2')
  [[ $cpu_count =~ ^[0-9]+$ ]] || cpu_count=2
  voice_threads=$(( (cpu_count + 1) / 2 ))
  [[ -n $(get_env TS6_VOICE_UDP_THREADS || true) ]] || set_env TS6_VOICE_UDP_THREADS "$(clamp "$cpu_count" 1 16)"
  [[ -n $(get_env TS6_VOICE_THREADS || true) ]] || set_env TS6_VOICE_THREADS "$(clamp "$voice_threads" 1 4)"
  [[ -n $(get_env TS6_COMMAND_THREADS || true) ]] || set_env TS6_COMMAND_THREADS "$(clamp "$voice_threads" 1 4)"
  [[ -n $(get_env TS6_QUERY_POOL_SIZE || true) ]] || set_env TS6_QUERY_POOL_SIZE "$(clamp "$cpu_count" 1 4)"
}

set_env() {
  local key=$1 value=$2 tmp
  mkdir -p "$INSTALL_ROOT"
  touch "$ENV_FILE"
  tmp=$(mktemp)
  awk -v k="$key" -v v="$value" '
    BEGIN { found=0 }
    index($0, k "=")==1 { print k "=" v; found=1; next }
    { print }
    END { if (!found) print k "=" v }
  ' "$ENV_FILE" > "$tmp"
  install -m 600 "$tmp" "$ENV_FILE"
  rm -f "$tmp"
}

get_env() {
  local key=$1
  [[ -f $ENV_FILE ]] || return 1
  sed -n "s/^${key}=//p" "$ENV_FILE" | tail -n 1
}

ensure_env_defaults() {
  local key
  mkdir -p "$INSTALL_ROOT"
  touch "$ENV_FILE"
  chmod 600 "$ENV_FILE"
  for key in "${!DEFAULTS[@]}"; do
    [[ -n $(get_env "$key" || true) ]] || set_env "$key" "${DEFAULTS[$key]}"
  done
  [[ -n $(get_env JWT_SECRET || true) ]] || set_env JWT_SECRET "$(random_secret)"
  [[ -n $(get_env ENCRYPTION_KEY || true) ]] || set_env ENCRYPTION_KEY "$(random_secret)"
  [[ -n $(get_env SIDECAR_SECRET || true) ]] || set_env SIDECAR_SECRET "$(random_secret)"
  [[ -n $(get_env TS6_ADMIN_PASSWORD || true) ]] || set_env TS6_ADMIN_PASSWORD "$(random_secret)"
  [[ -n $(get_env TS6_API_KEY || true) ]] || set_env TS6_API_KEY "$(random_secret)"
  [[ -n $(get_env TS3_ADMIN_PASSWORD || true) ]] || set_env TS3_ADMIN_PASSWORD "$(random_secret)"
  [[ -n $(get_env TS3_IMAGE || true) ]] || set_env TS3_IMAGE "$DEFAULT_TS3_IMAGE"
  [[ -n $(get_env TS6_IMAGE || true) ]] || set_env TS6_IMAGE "$DEFAULT_TS6_IMAGE"
  [[ -n $(get_env TEASPEAK_IMAGE || true) ]] || set_env TEASPEAK_IMAGE "$HARDENED_TEASPEAK_IMAGE"
  [[ -n $(get_env MANAGER_IMAGE || true) ]] || set_env MANAGER_IMAGE "$DEFAULT_MANAGER_IMAGE"
  [[ -n $(get_env MANAGER_SOURCE_REF || true) ]] || set_env MANAGER_SOURCE_REF "$DEFAULT_MANAGER_SOURCE_REF"
  [[ -n $(get_env COMPOSE_PROJECT_NAME || true) ]] || set_env COMPOSE_PROJECT_NAME "$PROJECT_NAME"
  [[ -n $(get_env QUERY_BIND_IP || true) ]] || set_env QUERY_BIND_IP "127.0.0.1"
  [[ -n $(get_env MANAGER_BIND_IP || true) ]] || set_env MANAGER_BIND_IP "127.0.0.1"
  detect_performance_defaults
}

install_docker() {
  if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
    ok "Docker ve Compose zaten kurulu."
    return
  fi

  [[ $OS_FAMILY != windows ]] || die "Docker Desktop çalışmıyor. Docker Desktop'ı başlatıp tekrar deneyin."

  check_os
  info "Docker resmi APT deposundan kuruluyor..."
  apt-get update
  apt-get install -y ca-certificates curl gnupg openssl iproute2
  install -m 0755 -d /etc/apt/keyrings
  # shellcheck disable=SC1091
  . /etc/os-release
  curl -fsSL "https://download.docker.com/linux/${ID}/gpg" -o /etc/apt/keyrings/docker.asc
  chmod a+r /etc/apt/keyrings/docker.asc
  local arch codename
  arch=$(dpkg --print-architecture)
  codename=${VERSION_CODENAME:-}
  [[ -n $codename ]] || die "Dağıtım kod adı bulunamadı."
  printf 'deb [arch=%s signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/%s %s stable\n' \
    "$arch" "$ID" "$codename" > /etc/apt/sources.list.d/docker.list
  apt-get update
  apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
  systemctl enable --now docker
  ok "Docker kurulumu tamamlandı."
}

write_compose() {
  ensure_env_defaults
  mkdir -p "$INSTALL_ROOT"
  cat > "${COMPOSE_FILE}.tmp" <<'YAML'
name: ${COMPOSE_PROJECT_NAME}

x-logging: &default-logging
  driver: json-file
  options:
    max-size: "10m"
    max-file: "3"

services:
  ts3:
    image: ${TS3_IMAGE}
    platform: linux/amd64
    init: true
    cap_drop: ["ALL"]
    security_opt: ["no-new-privileges:true"]
    profiles: ["ts3"]
    restart: ${TS3_RESTART_POLICY:-unless-stopped}
    environment:
      TS3SERVER_LICENSE: accept
      TS3SERVER_QUERY_PROTOCOLS: raw,http
      TS3SERVER_SERVERADMIN_PASSWORD: "${TS3_ADMIN_PASSWORD}"
      TS3SERVER_DEFAULT_VOICE_PORT: "${TS3_VOICE_PORT}"
      TS3SERVER_FILETRANSFER_PORT: "${TS3_FILE_PORT}"
    ports:
      - "${TS3_VOICE_PORT}:${TS3_VOICE_PORT}/udp"
      - "${QUERY_BIND_IP}:${TS3_QUERY_PORT}:10011/tcp"
      - "${QUERY_BIND_IP}:${TS3_WEBQUERY_PORT}:10080/tcp"
      - "${TS3_FILE_PORT}:${TS3_FILE_PORT}/tcp"
    volumes:
      - ts3-data:/var/ts3server
    networks: [teamhub]
    logging: *default-logging
    stop_grace_period: 30s
    healthcheck:
      test: ["CMD-SHELL", "nc -z 127.0.0.1 10011"]
      interval: 30s
      timeout: 5s
      retries: 5
      start_period: 20s

  ts6:
    image: ${TS6_IMAGE}
    profiles: ["ts6"]
    init: true
    cap_drop: ["ALL"]
    security_opt: ["no-new-privileges:true"]
    restart: ${TS6_RESTART_POLICY:-unless-stopped}
    environment:
      TSSERVER_LICENSE_ACCEPTED: accept
      TSSERVER_DEFAULT_PORT: "${TS6_VOICE_PORT}"
      TSSERVER_FILE_TRANSFER_PORT: "${TS6_FILE_PORT}"
      TSSERVER_QUERY_HTTP_ENABLED: "true"
      TSSERVER_QUERY_HTTP_ALLOW_GUEST: "0"
      TSSERVER_QUERY_HTTP_PORT: "10080"
      TSSERVER_QUERY_SSH_ENABLED: "true"
      TSSERVER_QUERY_SSH_ALLOW_GUEST: "0"
      TSSERVER_QUERY_SSH_PORT: "10022"
      TSSERVER_QUERY_ADMIN_PASSWORD: "${TS6_ADMIN_PASSWORD}"
      TSSERVER_QUERY_ADMIN_API_KEY: "${TS6_API_KEY}"
      TSSERVER_VOICE_UDP_THREADS: "${TS6_VOICE_UDP_THREADS}"
      TSSERVER_VOICE_THREADS: "${TS6_VOICE_THREADS}"
      TSSERVER_COMMAND_THREADS: "${TS6_COMMAND_THREADS}"
      TSSERVER_QUERY_POOL_SIZE: "${TS6_QUERY_POOL_SIZE}"
    ports:
      - "${TS6_VOICE_PORT}:${TS6_VOICE_PORT}/udp"
      - "${QUERY_BIND_IP}:${TS6_QUERY_SSH_PORT}:10022/tcp"
      - "${QUERY_BIND_IP}:${TS6_QUERY_HTTP_PORT}:10080/tcp"
      - "${TS6_FILE_PORT}:${TS6_FILE_PORT}/tcp"
    volumes:
      - ts6-data:/var/tsserver
    networks: [teamhub]
    logging: *default-logging
    stop_grace_period: 45s
  teaspeak:
    image: ${TEASPEAK_IMAGE}
    platform: linux/amd64
    profiles: ["teaspeak"]
    init: true
    user: "4242:4242"
    cap_drop: ["ALL"]
    security_opt: ["no-new-privileges:true"]
    restart: ${TEASPEAK_RESTART_POLICY:-unless-stopped}
    command:
      - "-Pgeneral.database.url=sqlite://database/TeaData.sqlite"
      - "-Pgeneral.license=${TEASPEAK_LICENSE:-none}"
    ports:
      - "${TEASPEAK_VOICE_PORT}:9987/tcp"
      - "${TEASPEAK_VOICE_PORT}:9987/udp"
      - "${QUERY_BIND_IP}:${TEASPEAK_QUERY_PORT}:10101/tcp"
      - "${TEASPEAK_FILE_PORT}:30303/tcp"
    volumes:
      - teaspeak-logs:/ts/logs
      - teaspeak-database:/ts/database
      - teaspeak-files:/ts/files
      - teaspeak-certs:/ts/certs
      - teaspeak-config:/ts/config
      - teaspeak-crash-dumps:/ts/crash_dumps
    networks: [teamhub]
    logging: *default-logging
    stop_grace_period: 45s
    healthcheck:
      test: ["CMD-SHELL", "timeout 3 bash -c '</dev/tcp/127.0.0.1/10101' && awk '$$2 ~ /:2703$$/ && $$4 == \"07\" { found=1 } END { exit !found }' /proc/net/udp /proc/net/udp6"]
      interval: 10s
      timeout: 5s
      retries: 9
      start_period: 15s

  manager:
    image: ${MANAGER_IMAGE}
    profiles: ["manager"]
    init: true
    user: "10001:10001"
    cap_drop: ["ALL"]
    security_opt: ["no-new-privileges:true"]
    read_only: true
    tmpfs:
      - /tmp:rw,noexec,nosuid,nodev,size=512m
      - /home/teamhub/.cache:rw,noexec,nosuid,nodev,size=256m
    restart: ${MANAGER_RESTART_POLICY:-unless-stopped}
    environment:
      NODE_ENV: production
      PORT: "3001"
      DATABASE_URL: file:/app/packages/backend/data/ts6webui.db
      JWT_SECRET: "${JWT_SECRET}"
      ENCRYPTION_KEY: "${ENCRYPTION_KEY}"
      SIDECAR_SECRET: "${SIDECAR_SECRET}"
      JWT_ACCESS_EXPIRY: 15m
      JWT_REFRESH_EXPIRY: 7d
      FRONTEND_URL: "${FRONTEND_URL:-http://localhost:3000}"
      MUSIC_DIR: /data/music
      SIDECAR_URL: http://127.0.0.1:9800
      SIDECAR_PORT: "9800"
    ports:
      - "${MANAGER_BIND_IP}:${MANAGER_PORT}:8080/tcp"
    volumes:
      - manager-data:/app/packages/backend/data
      - manager-music:/data/music
    networks: [teamhub]
    logging: *default-logging
    stop_grace_period: 45s

networks:
  teamhub:
    name: ${COMPOSE_PROJECT_NAME}

volumes:
  ts3-data:
  ts6-data:
  teaspeak-logs:
  teaspeak-database:
  teaspeak-files:
  teaspeak-certs:
  teaspeak-config:
  teaspeak-crash-dumps:
  manager-data:
  manager-music:
YAML
  install -m 644 "${COMPOSE_FILE}.tmp" "$COMPOSE_FILE"
  rm -f "${COMPOSE_FILE}.tmp"
}

configure_restart_policy() {
  local component=$1 key value
  key="${component^^}_RESTART_POLICY"
  if confirm "${component^^} sunucu yeniden başlayınca otomatik başlasın mı?" E; then
    value="unless-stopped"
  else
    value="no"
  fi
  set_env "$key" "$value"
}

install_ts3() {
  install_docker
  handle_existing_component ts3 && return
  ensure_amd64_emulation
  ensure_env_defaults
  prepare_ts3_image
  set_env TS3_VOICE_PORT "$(ask_port TS3_VOICE_PORT 'TS3 ses UDP portu' udp)"
  set_env TS3_QUERY_PORT "$(ask_port TS3_QUERY_PORT 'TS3 ServerQuery TCP portu' tcp)"
  set_env TS3_WEBQUERY_PORT "$(ask_port TS3_WEBQUERY_PORT 'TS3 WebQuery HTTP TCP portu' tcp)"
  set_env TS3_FILE_PORT "$(ask_port TS3_FILE_PORT 'TS3 dosya aktarım TCP portu' tcp)"
  configure_restart_policy ts3
  write_compose
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile ts3 up -d ts3
  generate_ts3_api_key || warn "TS3 API anahtarı otomatik üretilemedi; log ve ServerQuery erişimini kontrol edin."
  sync_manager_connections
  ok "TS3 ve HTTP WebQuery kuruldu. Yönetim bilgileri ${ENV_FILE} içinde saklandı."
}

install_teaspeak() {
  local license_code
  install_docker
  if [[ $(docker_platform) == linux/arm64 ]]; then
    die "TeaSpeak 1.5.6'nın resmî ARM64 sunucu paketi yoktur; amd64 emülasyonunda UDP ses servisi güvenilir çalışmaz. Bu sunucuda yerel ARM64 destekli TS6 seçeneğini kullanın veya TeaSpeak için x86_64/amd64 bir sunucu seçin."
  fi
  handle_existing_component teaspeak && return
  ensure_env_defaults
  prepare_teaspeak_image
  read -r -s -p "TeaSpeak lisans kodu (yoksa boş bırakın): " license_code
  printf '\n'
  set_env TEASPEAK_LICENSE "${license_code:-none}"
  unset license_code
  set_env TEASPEAK_VOICE_PORT "$(ask_port TEASPEAK_VOICE_PORT 'TeaSpeak ses TCP/UDP portu' udp)"
  set_env TEASPEAK_QUERY_PORT "$(ask_port TEASPEAK_QUERY_PORT 'TeaSpeak Query TCP portu' tcp)"
  set_env TEASPEAK_FILE_PORT "$(ask_port TEASPEAK_FILE_PORT 'TeaSpeak dosya aktarım TCP portu' tcp)"
  configure_restart_policy teaspeak
  write_compose
  if ! docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile teaspeak up -d --wait --wait-timeout 120 teaspeak; then
    compose_cmd logs --tail=120 teaspeak || true
    die "TeaSpeak sağlıklı duruma geçemedi; yukarıdaki logları kontrol edin."
  fi
  ok "TeaSpeak kuruldu. Query parolası ve yönetici anahtarı aşağıdaki ilk başlatma logundadır:"
  compose_cmd logs --tail=80 teaspeak
}

prepare_teaspeak_image() {
  info "SHA-256 doğrulamalı TeaSpeak 1.5.6 imajı hazırlanıyor..."
  docker build --platform linux/amd64 \
    -f "${SCRIPT_DIR}/docker/teaspeak-hardened.Dockerfile" \
    -t "$HARDENED_TEASPEAK_IMAGE" "${SCRIPT_DIR}/docker"
  set_env TEASPEAK_IMAGE "$HARDENED_TEASPEAK_IMAGE"
}

generate_ts3_api_key() {
  local password output api_key
  password=$(get_env TS3_ADMIN_PASSWORD)
  info "TS3 WebQuery API anahtarı üretiliyor..."
  for _ in {1..20}; do
    output=$(printf 'login client_login_name=serveradmin client_login_password=%s\napikeyadd scope=manage lifetime=0\nquit\n' "$password" \
      | docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" exec -T ts3 sh -c 'nc 127.0.0.1 10011' 2>/dev/null || true)
    api_key=$(printf '%s\n' "$output" | sed -n 's/.*apikey=\([^ ]*\).*/\1/p' | tail -n 1)
    if [[ -n $api_key ]]; then
      set_env TS3_API_KEY "$api_key"
      ok "TS3 WebQuery API anahtarı oluşturuldu."
      return 0
    fi
    sleep 2
  done
  return 1
}

install_ts6() {
  install_docker
  handle_existing_component ts6 && return
  ensure_env_defaults
  prepare_ts6_image
  set_env TS6_VOICE_PORT "$(ask_port TS6_VOICE_PORT 'TS6 ses UDP portu' udp)"
  set_env TS6_QUERY_SSH_PORT "$(ask_port TS6_QUERY_SSH_PORT 'TS6 Query SSH TCP portu' tcp)"
  set_env TS6_QUERY_HTTP_PORT "$(ask_port TS6_QUERY_HTTP_PORT 'TS6 WebQuery HTTP TCP portu' tcp)"
  set_env TS6_FILE_PORT "$(ask_port TS6_FILE_PORT 'TS6 dosya aktarım TCP portu' tcp)"
  configure_restart_policy ts6
  write_compose
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile ts6 up -d ts6
  sync_manager_connections
  ok "TS6 kuruldu. Yönetim bilgileri sadece ${ENV_FILE} içinde (izin 600)."
}

prepare_ts3_image() {
  local hardened_image="teamspeak-managment/ts3-hardened:3.13.8"
  info "Güvenlik güncellemeleri uygulanmış TS3 imajı hazırlanıyor..."
  docker build --platform linux/amd64 \
    --build-arg "BASE_IMAGE=${DEFAULT_TS3_IMAGE}" \
    -f "${SCRIPT_DIR}/docker/ts3-hardened.Dockerfile" \
    -t "$hardened_image" "${SCRIPT_DIR}/docker"
  set_env TS3_IMAGE "$hardened_image"
}

prepare_ts6_image() {
  local hardened_image="teamspeak-managment/ts6-hardened:6.0.0-beta13.1"
  info "Güvenlik güncellemeleri uygulanmış TS6 imajı hazırlanıyor..."
  docker build \
    --build-arg "BASE_IMAGE=${DEFAULT_TS6_IMAGE}" \
    -f "${SCRIPT_DIR}/docker/ts6-hardened.Dockerfile" \
    -t "$hardened_image" "${SCRIPT_DIR}/docker"
  set_env TS6_IMAGE "$hardened_image"
}

install_manager() {
  install_docker
  handle_existing_component manager && return
  ensure_env_defaults
  prepare_manager_image
  local port public_host bind_ip access_mode
  port=$(ask_port MANAGER_PORT 'Manager web TCP portu' tcp)
  set_env MANAGER_PORT "$port"
  public_host=$(detect_public_host)
  access_mode="tunnel"
  if confirm "Manager doğrudan ağdan erişilebilir olsun mu? (TLS/reverse proxy önerilir)" N; then
    bind_ip="0.0.0.0"
    access_mode="direct"
    set_env FRONTEND_URL "http://${public_host}:${port}"
  else
    bind_ip="127.0.0.1"
    set_env FRONTEND_URL "http://localhost:${port}"
  fi
  set_env MANAGER_BIND_IP "$bind_ip"
  configure_restart_policy manager
  write_compose
  prepare_manager_volume_permissions
  configure_manager_admin
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile manager up -d manager
  sync_manager_connections
  if [[ $access_mode == direct ]]; then
    ok "Manager kuruldu: http://${public_host}:${port}/login"
    warn "Panel ağda açık. Güvenlik duvarı ile sınırlandırın ve TLS reverse proxy kullanın."
  else
    ok "Manager yalnız sunucu üzerinde dinliyor: http://127.0.0.1:${port}/login"
    info "Windows PowerShell'de tünel açın: ssh -L ${port}:127.0.0.1:${port} KULLANICI@${public_host}"
    info "Sonra bilgisayarınızda açın: http://127.0.0.1:${port}/login"
  fi
  music_bot_guide
}

prepare_manager_image() {
  local source_ref arch tmp base_image
  source_ref=$(get_env MANAGER_SOURCE_REF || printf '%s' "$DEFAULT_MANAGER_SOURCE_REF")
  arch=$(docker_arch)
  info "Teamspeak Managment ${APP_VERSION} paneli doğrulanmış kaynak koddan ${arch} için hazırlanıyor..."
  command -v curl >/dev/null 2>&1 || die "Kaynak derleme için curl gerekli."
  command -v tar >/dev/null 2>&1 || die "Kaynak derleme için tar gerekli."
  command -v sha256sum >/dev/null 2>&1 || die "Kaynak doğrulama için sha256sum gerekli."
  tmp=$(mktemp -d)
  curl -fsSL "https://github.com/uniskela/ts6-manager/archive/${source_ref}.tar.gz" -o "${tmp}/manager.tar.gz"
  printf '%s  %s\n' "$DEFAULT_MANAGER_SOURCE_SHA256" "${tmp}/manager.tar.gz" | sha256sum -c -
  mkdir -p "${tmp}/src"
  tar -xzf "${tmp}/manager.tar.gz" -C "${tmp}/src" --strip-components=1
  cp -R "${SCRIPT_DIR}/manager-overlay/." "${tmp}/src/"
  docker build -f "${tmp}/src/Dockerfile.all-in-one" -t teamhub/ts6-manager:base "${tmp}/src"
  rm -rf -- "$tmp"
  base_image="teamhub/ts6-manager:base"

  info "Root olmayan, yetkileri azaltılmış Manager imajı hazırlanıyor..."
  docker build \
    --build-arg "BASE_IMAGE=${base_image}" \
    -f "${SCRIPT_DIR}/docker/manager-hardened.Dockerfile" \
    -t "$HARDENED_MANAGER_IMAGE" "$SCRIPT_DIR"
  set_env MANAGER_IMAGE "$HARDENED_MANAGER_IMAGE"
  ok "Manager güvenlik yamasıyla ${arch} için hazırlandı."
}

prepare_manager_volume_permissions() {
  info "Manager veri volume izinleri root olmayan çalışma kullanıcısına hazırlanıyor..."
  docker run --rm --user 0:0 \
    -v "${PROJECT_NAME}_manager-data:/app/packages/backend/data" \
    -v "${PROJECT_NAME}_manager-music:/data/music" \
    "$HARDENED_MANAGER_IMAGE" \
    sh -c 'chown -R 10001:10001 /app/packages/backend/data /data/music'
}

manager_offline_cli() {
  docker run --rm -i --network none --user 10001:10001 \
    --cap-drop ALL --security-opt no-new-privileges:true --read-only --tmpfs /tmp \
    --env-file "$ENV_FILE" \
    -e DATABASE_URL=file:/app/packages/backend/data/ts6webui.db \
    -v "${PROJECT_NAME}_manager-data:/app/packages/backend/data" \
    --workdir /app/packages/backend --entrypoint node \
    "$HARDENED_MANAGER_IMAGE" dist/bootstrap/installer-cli.js "$@"
}

configure_manager_admin() {
  local state username password repeated
  state=$(manager_offline_cli admin-state)
  if [[ $state != new ]]; then
    info "Manager yönetici hesabı mevcut veya hazırlanmış; hesabınız ve parolanız korunuyor."
    return
  fi
  while true; do
    read -r -p "Manager yönetici kullanıcı adı [admin]: " username
    username=${username:-admin}
    if [[ $username =~ ^[a-zA-Z0-9_.-]{3,64}$ ]]; then break; fi
    warn "Kullanıcı adı 3–64 karakter olmalı; harf, rakam, nokta, tire ve alt çizgi kullanılabilir."
  done
  while true; do
    read -r -s -p "Manager yönetici parolası (8–72 karakter; büyük/küçük harf ve rakam): " password
    printf '\n'
    read -r -s -p "Parolayı tekrar girin: " repeated
    printf '\n'
    if [[ $password == "$repeated" && ${#password} -ge 8 && ${#password} -le 72 \
      && $password =~ [A-Z] && $password =~ [a-z] && $password =~ [0-9] ]]; then
      if printf '%s\0%s\0' "$username" "$password" | manager_offline_cli prepare-admin >/dev/null; then break; fi
    fi
    warn "Parolalar aynı olmalı ve belirtilen koşulları sağlamalı (UTF-8 en fazla 72 bayt)."
  done
  unset password repeated
  ok "Yönetici hesabı ilk açılış için hazırlandı; parola düz metin olarak saklanmadı."
}

wait_manager_ready() {
  local manager_id=$1
  for _ in {1..60}; do
    if docker exec "$manager_id" node -e 'fetch("http://127.0.0.1:3001/api/health", {signal:AbortSignal.timeout(2000)}).then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))' >/dev/null 2>&1; then return 0; fi
    sleep 2
  done
  return 1
}

sync_manager_connections() {
  local manager_id project env_path kind server_id host key result network changed=0
  local -a env_paths=("$ENV_FILE" "${DEFAULT_INSTALL_ROOT}/.env")
  if [[ $OS_FAMILY == windows ]]; then
    env_paths+=("${HOME}/.teamhub-2/.env")
  else
    env_paths+=("/opt/teamhub-2/.env")
  fi
  manager_id=$(docker ps -q --filter "label=com.docker.compose.project=${PROJECT_NAME}" --filter label=com.docker.compose.service=manager | head -n 1)
  if [[ -z $manager_id ]]; then
    manager_id=$(docker ps -q --filter label=com.docker.compose.project=teamhub --filter label=com.docker.compose.service=manager | head -n 1)
  fi
  if [[ -z $manager_id ]]; then
    manager_id=$(docker ps -q --filter label=com.docker.compose.project=teamhub2 --filter label=com.docker.compose.service=manager | head -n 1)
  fi
  if [[ -z $manager_id ]]; then
    info "Manager henüz çalışmıyor; sunucu bilgileri Manager kurulduğunda otomatik eşleştirilecek."
    return
  fi
  if ! wait_manager_ready "$manager_id"; then warn "Manager hazır değil; otomatik bağlantı kaydı tamamlanamadı. Manager güncellemesini yeniden çalıştırın."; return; fi
  for env_path in "${env_paths[@]}"; do
    [[ -r $env_path ]] || continue
    project=$(sed -n 's/^COMPOSE_PROJECT_NAME=//p' "$env_path" | tail -n 1)
    [[ $project =~ ^[a-zA-Z0-9][a-zA-Z0-9_-]*$ ]] || continue
    for kind in ts3 ts6; do
      server_id=$(docker ps -aq --filter "label=com.docker.compose.project=${project}" --filter "label=com.docker.compose.service=${kind}" | head -n 1)
      [[ -n $server_id ]] || continue
      key=$(sed -n "s/^${kind^^}_API_KEY=//p" "$env_path" | tail -n 1)
      if [[ -z $key ]]; then warn "${project}/${kind}: API anahtarı bulunamadı; bağlantı eklenmedi."; continue; fi
      network=$(docker network inspect "$project" --format '{{.Name}}' 2>/dev/null || true)
      [[ -n $network ]] || { warn "${project}/${kind}: Docker ağı bulunamadı."; continue; }
      if ! docker inspect "$manager_id" --format '{{range $name, $network := .NetworkSettings.Networks}}{{println $name}}{{end}}' | grep -Fxq "$network"; then
        docker network connect "$network" "$manager_id" || { warn "Manager sunucu ağına bağlanamadı."; continue; }
      fi
      host=$(docker inspect "$server_id" --format '{{.Name}}')
      host=${host#/}
      if result=$(printf '%s\0%s\0%s\0' "${kind^^} · ${project}" "$host" "$key" \
        | docker exec -i --workdir /app/packages/backend "$manager_id" node dist/bootstrap/installer-cli.js sync-connection); then
        [[ $result != changed ]] || changed=1
        ok "${project}/${kind}: Manager bağlantı kaydı hazır."
      else
        warn "${project}/${kind}: otomatik kayıt başarısız; mevcut bağlantılar korundu. Eski Manager kullanılıyorsa önce güncelleyin."
      fi
    done
  done
  unset key
  if (( changed )); then
    info "Yeni bağlantıların yüklenmesi için Manager kısa süre yeniden başlatılıyor."
    docker restart "$manager_id" >/dev/null
    wait_manager_ready "$manager_id" || die "Manager yeniden başlatıldı ancak sağlık kontrolü başarısız. Logları kontrol edin."
  fi
}

music_bot_guide() {
  local manager_port manager_bind public_host ts6_http ts3_http manager_step
  manager_port=$(get_env MANAGER_PORT || printf '3000')
  manager_bind=$(get_env MANAGER_BIND_IP || printf '127.0.0.1')
  public_host=$(detect_public_host)
  ts6_http=$(get_env TS6_QUERY_HTTP_PORT || printf '10081')
  ts3_http=$(get_env TS3_WEBQUERY_PORT || printf '10080')
  if [[ $manager_bind == 127.0.0.1 ]]; then
    manager_step="Windows PowerShell'de 'ssh -L ${manager_port}:127.0.0.1:${manager_port} KULLANICI@${public_host}' çalıştır; ardından http://127.0.0.1:${manager_port}/login adresini aç."
  else
    manager_step="Tarayıcıda http://${public_host}:${manager_port}/login adresini aç (üretimde TLS reverse proxy kullan)."
  fi
  cat <<EOF

Müzik botu temiz kurulum adımları
---------------------------------
1. ${manager_step}
2. Terminalde belirlediğin yönetici hesabıyla giriş yap. Mevcut hesap varsa parolası değişmez.
3. Kurulan TS3/TS6 bağlantıları sunucu seçicisinde otomatik görünür.
   - Uzak sunucular Ayarlar > Bağlantılar bölümünden ayrıca eklenebilir.
   - Dış erişimde TS6 WebQuery portu ${ts6_http}, TS3 WebQuery portu ${ts3_http} olur.
   Bot Hub > New bot ile sunucusunu, adını, kanalını ve otomatik başlatma tercihini seç.
4. Bot bağlandıktan sonra !play, !radio, !vol ve !skip komutlarını kanalda dene.

Hiçbir eski bot kimliği, token, veritabanı veya parola bu kuruluma aktarılmaz.
EOF
}

install_full_stack() {
  info "Tam paket sırasıyla TS6, Manager ve müzik botu hazırlığını kurar."
  install_ts6
  install_manager
}

update_manager() {
  [[ -f $COMPOSE_FILE && -f $ENV_FILE ]] || die "Güncellenecek bir TeamHub kurulumu bulunamadı."
  info "Manager verileri yedekleniyor; veritabanı ve müzikler korunacak."
  backup_data
  set_env MANAGER_IMAGE "$DEFAULT_MANAGER_IMAGE"
  set_env MANAGER_SOURCE_REF "$DEFAULT_MANAGER_SOURCE_REF"
  prepare_manager_image
  write_compose
  prepare_manager_volume_permissions
  configure_manager_admin
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile manager up -d manager
  sync_manager_connections
  ok "TeamSpeak Manager güncellendi; mevcut veritabanı ve müzik volume'ları korundu."
}

compose_cmd() {
  [[ -f $COMPOSE_FILE && -f $ENV_FILE ]] || die "Henüz kurulum bulunamadı."
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" "$@"
}

show_status() {
  compose_cmd ps -a
}

show_logs() {
  local service
  service=$(prompt_value "Servis (ts3/ts6/teaspeak/manager)" "manager")
  [[ $service =~ ^(ts3|ts6|teaspeak|manager)$ ]] || die "Geçersiz servis."
  service_exists "$service" || die "$service kurulu değil."
  compose_cmd logs --tail=200 "$service"
}

manage_service() {
  local service action
  service=$(prompt_value "Servis (ts3/ts6/teaspeak/manager)" "manager")
  [[ $service =~ ^(ts3|ts6|teaspeak|manager)$ ]] || die "Geçersiz servis."
  service_exists "$service" || die "$service kurulu değil; önce kurulum menüsünden yükleyin."
  action=$(prompt_value "İşlem (start/stop/restart)" "restart")
  [[ $action =~ ^(start|stop|restart)$ ]] || die "Geçersiz işlem."
  if [[ $action == start ]] && service_running "$service"; then
    info "$service zaten çalışıyor; işlem yapılmadı."
    return
  fi
  if [[ $action == stop ]] && ! service_running "$service"; then
    info "$service zaten durmuş; işlem yapılmadı."
    return
  fi
  compose_cmd "$action" "$service"
  if [[ $action == stop ]]; then
    service_running "$service" && die "$service durdurulamadı."
  else
    service_running "$service" || die "$service başlatılamadı; logları kontrol edin."
  fi
  ok "$service: $action tamamlandı."
}

backup_data() {
  [[ -f $COMPOSE_FILE && -f $ENV_FILE ]] || die "Henüz kurulum bulunamadı."
  local backup_dir archive backup_password backup_password_confirm service
  local -a running_services=() volume_mounts=()
  backup_dir="${INSTALL_ROOT}/backups"
  archive="${backup_dir}/teamhub-$(date +%Y%m%d-%H%M%S).tar.gz.enc"
  mkdir -p "$backup_dir"
  while IFS= read -r service; do
    case "$service" in
      ts3|ts6|teaspeak|manager) running_services+=("$service") ;;
    esac
  done < <(docker ps --filter "label=com.docker.compose.project=${PROJECT_NAME}" --format '{{.Label "com.docker.compose.service"}}' | sort -u)

  volume_exists "${PROJECT_NAME}_ts3-data" && volume_mounts+=(-v "${PROJECT_NAME}_ts3-data:/data/ts3:ro")
  volume_exists "${PROJECT_NAME}_ts6-data" && volume_mounts+=(-v "${PROJECT_NAME}_ts6-data:/data/ts6:ro")
  volume_exists "${PROJECT_NAME}_teaspeak-database" && volume_mounts+=(-v "${PROJECT_NAME}_teaspeak-database:/data/teaspeak/database:ro")
  volume_exists "${PROJECT_NAME}_teaspeak-files" && volume_mounts+=(-v "${PROJECT_NAME}_teaspeak-files:/data/teaspeak/files:ro")
  volume_exists "${PROJECT_NAME}_teaspeak-config" && volume_mounts+=(-v "${PROJECT_NAME}_teaspeak-config:/data/teaspeak/config:ro")
  volume_exists "${PROJECT_NAME}_teaspeak-certs" && volume_mounts+=(-v "${PROJECT_NAME}_teaspeak-certs:/data/teaspeak/certs:ro")
  volume_exists "${PROJECT_NAME}_teaspeak-logs" && volume_mounts+=(-v "${PROJECT_NAME}_teaspeak-logs:/data/teaspeak/logs:ro")
  volume_exists "${PROJECT_NAME}_teaspeak-crash-dumps" && volume_mounts+=(-v "${PROJECT_NAME}_teaspeak-crash-dumps:/data/teaspeak/crash_dumps:ro")
  volume_exists "${PROJECT_NAME}_manager-data" && volume_mounts+=(-v "${PROJECT_NAME}_manager-data:/data/manager:ro")
  volume_exists "${PROJECT_NAME}_manager-music" && volume_mounts+=(-v "${PROJECT_NAME}_manager-music:/data/music:ro")

  read -r -s -p "Yedek şifresi (en az 16 karakter): " backup_password
  printf '\n'
  (( ${#backup_password} >= 16 )) || die "Yedek şifresi en az 16 karakter olmalıdır."
  read -r -s -p "Yedek şifresini tekrar girin: " backup_password_confirm
  printf '\n'
  [[ $backup_password == "$backup_password_confirm" ]] || die "Yedek şifreleri eşleşmiyor."

  if (( ${#running_services[@]} > 0 )); then
    info "Tutarlı yedek için yalnızca çalışan servisler durduruluyor: ${running_services[*]}"
    compose_cmd stop "${running_services[@]}"
  else
    info "Çalışan TeamHub servisi yok; veri doğrudan yedeklenecek."
  fi
  if ! docker run --rm \
      "${volume_mounts[@]}" \
      -v "${ENV_FILE}:/config/.env:ro" \
      -v "${COMPOSE_FILE}:/config/compose.yml:ro" \
      --platform "$(docker_platform)" "$ALPINE_IMAGE" sh -c "tar czf - -C / data config" \
      | openssl enc -aes-256-cbc -pbkdf2 -iter 600000 -salt \
          -out "$archive" -pass fd:3 3<<<"$backup_password"; then
    warn "Yedekleme başarısız oldu; servisler yeniden başlatılıyor."
    (( ${#running_services[@]} == 0 )) || compose_cmd start "${running_services[@]}" \
      || warn "Servisler otomatik başlayamadı; menüden durumlarını kontrol edin."
    rm -f -- "$archive"
    return 1
  fi
  (( ${#running_services[@]} == 0 )) || compose_cmd start "${running_services[@]}"
  unset backup_password backup_password_confirm
  chmod 600 "$archive"
  ok "Yedek oluşturuldu: $archive"
  warn "Şifreyi kaybederseniz yedek kurtarılamaz; arşivi sunucu dışında saklayın."
}

uninstall_component() {
  local service volume
  service=$(prompt_value "Kaldırılacak bileşen (ts3/ts6/teaspeak/manager)" "manager")
  [[ $service =~ ^(ts3|ts6|teaspeak|manager)$ ]] || die "Geçersiz bileşen."
  if ! service_exists "$service"; then
    warn "$service konteyneri kurulu değil; hiçbir konteyner silinmedi."
    case "$service" in
      ts3) volume="${PROJECT_NAME}_ts3-data" ;;
      ts6) volume="${PROJECT_NAME}_ts6-data" ;;
      teaspeak) volume="${PROJECT_NAME}_teaspeak-database" ;;
      manager) volume="${PROJECT_NAME}_manager-data" ;;
    esac
    if volume_exists "$volume"; then
      info "$service için kalıntı veri volume'u var. Güvenlik için otomatik silinmedi: $volume"
    fi
    return
  fi
  confirm "$service konteyneri kaldırılsın mı?" N || return
  compose_cmd rm -sf "$service"
  if confirm "$service verileri de kalıcı olarak silinsin mi?" N; then
    case "$service" in
      ts3) volume="${PROJECT_NAME}_ts3-data" ;;
      ts6) volume="${PROJECT_NAME}_ts6-data" ;;
      teaspeak)
        docker volume rm "${PROJECT_NAME}_teaspeak-logs" "${PROJECT_NAME}_teaspeak-database" "${PROJECT_NAME}_teaspeak-files" \
          "${PROJECT_NAME}_teaspeak-certs" "${PROJECT_NAME}_teaspeak-config" "${PROJECT_NAME}_teaspeak-crash-dumps" 2>/dev/null || true
        ok "TeaSpeak verileri silindi."
        return
        ;;
      manager)
        docker volume rm "${PROJECT_NAME}_manager-data" "${PROJECT_NAME}_manager-music" 2>/dev/null || true
        ok "Manager verileri silindi."
        return
        ;;
    esac
    docker volume rm "$volume" 2>/dev/null || true
    ok "$service verileri silindi."
  fi
}

show_header() {
  clear 2>/dev/null || true
  printf '\n%s v%s\n' "$APP_NAME" "$APP_VERSION"
  printf 'Kurulum: %s (%s)\n' "$PROJECT_NAME" "$INSTALL_ROOT"
  printf '%s\n' '=================================='
}

main_menu() {
  while true; do
    show_header
    cat <<'MENU'
1) Docker gereksinimlerini kur
2) TeamSpeak 3 sunucusu kur
3) TeaSpeak sunucusu kur
4) TeamSpeak 6 sunucusu kur
5) TS6 Manager kur
6) Müzik botu bağlantı yönergesini göster
7) Önerilen tam paket (TS6 + Manager + müzik botu hazırlığı)
8) TeamSpeak Manager'ı verileri koruyarak güncelle
9) Servis durumları
10) Logları göster
11) Servis başlat / durdur / yeniden başlat
12) Verileri yedekle
13) Bir bileşeni kaldır
0) Çıkış
MENU
    local choice
    read -r -p "Seçiminiz: " choice
    case "$choice" in
      1) install_docker ;;
      2) install_ts3 ;;
      3) install_teaspeak ;;
      4) install_ts6 ;;
      5) install_manager ;;
      6) ensure_env_defaults; music_bot_guide ;;
      7) install_full_stack ;;
      8) update_manager ;;
      9) show_status ;;
      10) show_logs ;;
      11) manage_service ;;
      12) backup_data ;;
      13) uninstall_component ;;
      0) exit 0 ;;
      *) warn "Geçersiz seçim." ;;
    esac
    printf '\n'
    read -r -p "Menüye dönmek için Enter..." _
  done
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  require_root
  check_os
  main_menu
fi
