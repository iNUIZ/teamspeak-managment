#!/usr/bin/env bash
set -Eeuo pipefail

readonly APP_NAME="Teamspeak Managment"
readonly APP_VERSION="0.3.0"
KERNEL_NAME="$(uname -s)"
readonly KERNEL_NAME
case "$KERNEL_NAME" in
  MINGW*|MSYS*|CYGWIN*) readonly OS_FAMILY="windows"; readonly DEFAULT_INSTALL_ROOT="${HOME}/.teamhub" ;;
  *)                    readonly OS_FAMILY="linux";   readonly DEFAULT_INSTALL_ROOT="/opt/teamhub" ;;
esac
readonly INSTALL_ROOT="${TEAMHUB_ROOT:-$DEFAULT_INSTALL_ROOT}"
readonly COMPOSE_FILE="${INSTALL_ROOT}/compose.yml"
readonly ENV_FILE="${INSTALL_ROOT}/.env"
readonly DEFAULT_TS3_IMAGE="teamspeak:3.13.8@sha256:6dfdfb22869adf50e1b66d024b360b786d57a85ea09e8e8fdb6cca23949b2813"
readonly DEFAULT_TS6_IMAGE="teamspeaksystems/teamspeak6-server:6.0.0-beta13.1@sha256:d845f803aecf842c2df034c959c23ff2462357bacafb3807500885c5ec170fa1"
readonly DEFAULT_MANAGER_IMAGE="ghcr.io/uniskela/ts6-manager/all-in-one:1.11.0@sha256:0ed720e3ee8c4635add4c8e028457aa693dd73ef49216572089c26bd72201840"
readonly DEFAULT_MANAGER_SOURCE_REF="6cb29eb6093ce39ca99699b7ead80d13edcf19bf"
readonly DEFAULT_MANAGER_SOURCE_SHA256="d3938e96afe8b232e21ec4dbf80b18963a6afa1f4e3196b810997d936f93836a"
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
  [[ -n $(get_env MANAGER_IMAGE || true) ]] || set_env MANAGER_IMAGE "$DEFAULT_MANAGER_IMAGE"
  [[ -n $(get_env MANAGER_SOURCE_REF || true) ]] || set_env MANAGER_SOURCE_REF "$DEFAULT_MANAGER_SOURCE_REF"
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
name: teamhub

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

  manager:
    image: ${MANAGER_IMAGE}
    profiles: ["manager"]
    init: true
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
      - "${MANAGER_BIND_IP}:${MANAGER_PORT}:80/tcp"
    volumes:
      - manager-data:/app/packages/backend/data
      - manager-music:/data/music
    networks: [teamhub]
    logging: *default-logging
    stop_grace_period: 45s

networks:
  teamhub:
    name: teamhub

volumes:
  ts3-data:
  ts6-data:
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
  if [[ $(uname -m) =~ ^(aarch64|arm64)$ ]]; then
    info "ARM üzerinde TS3 amd64 emülasyonu kontrol ediliyor..."
    docker run --rm --platform linux/amd64 "$ALPINE_IMAGE" true >/dev/null \
      || die "Docker amd64 emülasyonu etkin değil. Docker Desktop emülasyonunu veya Linux binfmt/QEMU desteğini etkinleştirin."
  fi
  ensure_env_defaults
  set_env TS3_VOICE_PORT "$(ask_port TS3_VOICE_PORT 'TS3 ses UDP portu' udp)"
  set_env TS3_QUERY_PORT "$(ask_port TS3_QUERY_PORT 'TS3 ServerQuery TCP portu' tcp)"
  set_env TS3_WEBQUERY_PORT "$(ask_port TS3_WEBQUERY_PORT 'TS3 WebQuery HTTP TCP portu' tcp)"
  set_env TS3_FILE_PORT "$(ask_port TS3_FILE_PORT 'TS3 dosya aktarım TCP portu' tcp)"
  configure_restart_policy ts3
  write_compose
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile ts3 up -d ts3
  generate_ts3_api_key || warn "TS3 API anahtarı otomatik üretilemedi; log ve ServerQuery erişimini kontrol edin."
  ok "TS3 ve HTTP WebQuery kuruldu. Yönetim bilgileri ${ENV_FILE} içinde saklandı."
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
  ensure_env_defaults
  set_env TS6_VOICE_PORT "$(ask_port TS6_VOICE_PORT 'TS6 ses UDP portu' udp)"
  set_env TS6_QUERY_SSH_PORT "$(ask_port TS6_QUERY_SSH_PORT 'TS6 Query SSH TCP portu' tcp)"
  set_env TS6_QUERY_HTTP_PORT "$(ask_port TS6_QUERY_HTTP_PORT 'TS6 WebQuery HTTP TCP portu' tcp)"
  set_env TS6_FILE_PORT "$(ask_port TS6_FILE_PORT 'TS6 dosya aktarım TCP portu' tcp)"
  configure_restart_policy ts6
  write_compose
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile ts6 up -d ts6
  ok "TS6 kuruldu. Yönetim bilgileri sadece ${ENV_FILE} içinde (izin 600)."
}

install_manager() {
  install_docker
  ensure_env_defaults
  prepare_manager_image
  local port public_host default_host
  port=$(ask_port MANAGER_PORT 'Manager web TCP portu' tcp)
  set_env MANAGER_PORT "$port"
  if confirm "Manager web arayüzü diğer cihazlardan erişilebilir olsun mu?" N; then
    set_env MANAGER_BIND_IP "0.0.0.0"
    default_host=$(hostname -I 2>/dev/null | awk '{print $1}' || true)
    [[ -n $default_host ]] || default_host="localhost"
    while true; do
      public_host=$(prompt_value "Manager için sunucu IP/alan adı (http:// yazmadan)" "$default_host")
      valid_public_host "$public_host" && break
      warn "Yalnız geçerli bir IPv4 veya alan adı girin; protokol, port, boşluk ve yol eklemeyin."
    done
    warn "Manager dış ağa açıldı. Güvenlik duvarı ve TLS reverse proxy yapılandırın."
  else
    set_env MANAGER_BIND_IP "127.0.0.1"
    public_host="localhost"
  fi
  set_env FRONTEND_URL "http://${public_host}:${port}"
  configure_restart_policy manager
  write_compose
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile manager up -d manager
  ok "Manager kuruldu: http://${public_host}:${port}/setup"
  music_bot_guide
}

prepare_manager_image() {
  local upstream source_ref arch tmp
  upstream=$(get_env MANAGER_IMAGE || printf '%s' "$DEFAULT_MANAGER_IMAGE")
  source_ref=$(get_env MANAGER_SOURCE_REF || printf '%s' "$DEFAULT_MANAGER_SOURCE_REF")
  arch=$(docker_arch)
  if [[ $arch =~ ^(amd64|x86_64)$ ]]; then
    set_env MANAGER_IMAGE "$upstream"
    return
  fi

  info "${arch} için hazır Manager imajı kontrol ediliyor..."
  if docker pull "$upstream" >/dev/null 2>&1; then
    set_env MANAGER_IMAGE "$upstream"
    ok "Manager için doğal ${arch} imajı bulundu."
    return
  fi

  warn "Hazır ${arch} Manager imajı bulunamadı; kaynak koddan doğal mimaride derlenecek."
  command -v curl >/dev/null 2>&1 || die "Kaynak derleme için curl gerekli."
  command -v tar >/dev/null 2>&1 || die "Kaynak derleme için tar gerekli."
  command -v sha256sum >/dev/null 2>&1 || die "Kaynak doğrulama için sha256sum gerekli."
  tmp=$(mktemp -d)
  curl -fsSL "https://github.com/uniskela/ts6-manager/archive/${source_ref}.tar.gz" -o "${tmp}/manager.tar.gz"
  printf '%s  %s\n' "$DEFAULT_MANAGER_SOURCE_SHA256" "${tmp}/manager.tar.gz" | sha256sum -c -
  mkdir -p "${tmp}/src"
  tar -xzf "${tmp}/manager.tar.gz" -C "${tmp}/src" --strip-components=1
  docker build -f "${tmp}/src/Dockerfile.all-in-one" -t teamhub/ts6-manager:local "${tmp}/src"
  rm -rf -- "$tmp"
  set_env MANAGER_IMAGE "teamhub/ts6-manager:local"
  ok "Manager ${arch} için yerel olarak derlendi."
}

music_bot_guide() {
  local manager_port ts6_http ts3_http
  manager_port=$(get_env MANAGER_PORT || printf '3000')
  ts6_http=$(get_env TS6_QUERY_HTTP_PORT || printf '10081')
  ts3_http=$(get_env TS3_WEBQUERY_PORT || printf '10080')
  cat <<EOF

Müzik botu temiz kurulum adımları
---------------------------------
1. Tarayıcıda http://SUNUCU_IP:${manager_port}/setup adresini açıp ilk admini oluştur.
2. Ayarlar > Connections bölümünde sunucuyu ekle.
   - Aynı paket içindeki TS6: host ts6, port 10080, anahtar ${ENV_FILE} içindeki TS6_API_KEY.
   - Aynı paket içindeki TS3: host ts3, port 10080, anahtar ${ENV_FILE} içindeki TS3_API_KEY.
   - Dış erişimde TS6 WebQuery portu ${ts6_http}, TS3 WebQuery portu ${ts3_http} olur.
3. Music Bots > Add Bot ile adını, kanalını ve otomatik başlatma tercihini seç.
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
  local manager_image
  [[ -f $COMPOSE_FILE && -f $ENV_FILE ]] || die "Güncellenecek bir TeamHub kurulumu bulunamadı."
  info "Manager verileri yedekleniyor; veritabanı ve müzikler korunacak."
  backup_data
  set_env MANAGER_IMAGE "$DEFAULT_MANAGER_IMAGE"
  set_env MANAGER_SOURCE_REF "$DEFAULT_MANAGER_SOURCE_REF"
  prepare_manager_image
  write_compose
  manager_image=$(get_env MANAGER_IMAGE)
  if [[ $manager_image != teamhub/* ]]; then
    docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile manager pull manager
  fi
  docker compose --env-file "$ENV_FILE" -f "$COMPOSE_FILE" --profile manager up -d manager
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
  service=$(prompt_value "Servis (ts3/ts6/manager)" "manager")
  [[ $service =~ ^(ts3|ts6|manager)$ ]] || die "Geçersiz servis."
  compose_cmd logs --tail=200 "$service"
}

manage_service() {
  local service action
  service=$(prompt_value "Servis (ts3/ts6/manager)" "manager")
  [[ $service =~ ^(ts3|ts6|manager)$ ]] || die "Geçersiz servis."
  action=$(prompt_value "İşlem (start/stop/restart)" "restart")
  [[ $action =~ ^(start|stop|restart)$ ]] || die "Geçersiz işlem."
  compose_cmd "$action" "$service"
  ok "$service: $action tamamlandı."
}

backup_data() {
  [[ -f $COMPOSE_FILE && -f $ENV_FILE ]] || die "Henüz kurulum bulunamadı."
  local backup_dir archive
  backup_dir="${INSTALL_ROOT}/backups"
  archive="${backup_dir}/teamhub-$(date +%Y%m%d-%H%M%S).tar.gz"
  mkdir -p "$backup_dir"
  info "Tutarlı yedek için çalışan servisler kısa süre durduruluyor..."
  compose_cmd stop
  if ! docker run --rm \
      -v teamhub_ts3-data:/data/ts3:ro \
      -v teamhub_ts6-data:/data/ts6:ro \
      -v teamhub_manager-data:/data/manager:ro \
      -v teamhub_manager-music:/data/music:ro \
      -v "$backup_dir:/backup" \
      "$ALPINE_IMAGE" sh -c "tar czf /backup/$(basename "$archive") -C / data"; then
    warn "Yedekleme başarısız oldu; servisler yeniden başlatılıyor."
    compose_cmd start || warn "Servisler otomatik başlayamadı; menüden durumlarını kontrol edin."
    return 1
  fi
  compose_cmd start
  chmod 600 "$archive"
  ok "Yedek oluşturuldu: $archive"
}

uninstall_component() {
  local service volume
  service=$(prompt_value "Kaldırılacak bileşen (ts3/ts6/manager)" "manager")
  [[ $service =~ ^(ts3|ts6|manager)$ ]] || die "Geçersiz bileşen."
  confirm "$service konteyneri kaldırılsın mı?" N || return
  compose_cmd rm -sf "$service"
  if confirm "$service verileri de kalıcı olarak silinsin mi?" N; then
    case "$service" in
      ts3) volume="teamhub_ts3-data" ;;
      ts6) volume="teamhub_ts6-data" ;;
      manager)
        docker volume rm teamhub_manager-data teamhub_manager-music 2>/dev/null || true
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
  printf '%s\n' '=================================='
}

main_menu() {
  while true; do
    show_header
    cat <<'MENU'
1) Docker gereksinimlerini kur
2) TeamSpeak 3 sunucusu kur
3) TeamSpeak 6 sunucusu kur
4) TS6 Manager kur
5) Müzik botu bağlantı yönergesini göster
6) Önerilen tam paket (TS6 + Manager + müzik botu hazırlığı)
7) TeamSpeak Manager'ı verileri koruyarak güncelle
8) Servis durumları
9) Logları göster
10) Servis başlat / durdur / yeniden başlat
11) Verileri yedekle
12) Bir bileşeni kaldır
0) Çıkış
MENU
    local choice
    read -r -p "Seçiminiz: " choice
    case "$choice" in
      1) install_docker ;;
      2) install_ts3 ;;
      3) install_ts6 ;;
      4) install_manager ;;
      5) ensure_env_defaults; music_bot_guide ;;
      6) install_full_stack ;;
      7) update_manager ;;
      8) show_status ;;
      9) show_logs ;;
      10) manage_service ;;
      11) backup_data ;;
      12) uninstall_component ;;
      0) exit 0 ;;
      *) warn "Geçersiz seçim." ;;
    esac
    printf '\n'
    read -r -p "Menüye dönmek için Enter..." _
  done
}

require_root
check_os
main_menu
