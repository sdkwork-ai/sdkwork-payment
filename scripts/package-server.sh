#!/usr/bin/env bash
# package-server.sh — canonical server release packager for sdkwork-payment.
#
# Produces the MODULE_BIN_SPEC.md §4.4 artifact for the standalone gateway:
#   sdkwork-payment-server-<version>-linux-<arch>.tar.gz (+ .sha256)
#
# Archive content follows PACKAGING_SPEC.md §5.2/§6:
#   bin/sdkwork-api-payment-standalone-gateway   stripped release binary (--help/--version capable)
#   install/install-ubuntu.sh                    idempotent installer (systemd unit, /opt layout, --version gate)
#   install/sdkwork-payment.service              systemd unit template
#   config/env/payment.env.example               environment template (no secrets, no env-baked values)
#   install-manifest.json                        machine-readable content manifest (generated digests)
#
# This script is the single packaging authority; bin/lib/module.sh's
# sdkwork_package_app delegates here and nothing else hand-rolls the archive.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CRATE_DIR="${REPO_ROOT}/crates/sdkwork-api-payment-standalone-gateway"
BINARY_NAME="sdkwork-api-payment-standalone-gateway"
PACKAGE_ID="sdkwork-payment-server"

OUT_DIR="${REPO_ROOT}/target/bin-packages"
SKIP_BUILD=0
REQUESTED_VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT_DIR="$2"; shift 2 ;;
    --out=*) OUT_DIR="${1#--out=}"; shift ;;
    --version) REQUESTED_VERSION="$2"; shift 2 ;;
    --version=*) REQUESTED_VERSION="${1#--version=}"; shift ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    *) echo "unknown argument '$1' (usage: package-server.sh [--out <dir>] [--version <v>] [--skip-build])" >&2; exit 64 ;;
  esac
  shift || true
done

log() { printf '[package-server] %s\n' "$*"; }

if [[ "${SKIP_BUILD}" -ne 1 ]]; then
  log "building release gateway (cargo build --release -p sdkwork-api-payment-standalone-gateway)"
  (cd "${REPO_ROOT}" && cargo build --release -p sdkwork-api-payment-standalone-gateway)
else
  log "skipping build (--skip-build); reusing existing release binary"
fi

BINARY_SRC="${REPO_ROOT}/target/release/${BINARY_NAME}"
[[ -f "${BINARY_SRC}" ]] || { echo "release binary missing: ${BINARY_SRC}" >&2; exit 1; }

if [[ -z "${REQUESTED_VERSION}" ]]; then
  # Single source of truth: the binary's own pre-init information command.
  REQUESTED_VERSION="$("${BINARY_SRC}" --version | awk '{print $NF}')"
fi
[[ -n "${REQUESTED_VERSION}" ]] || { echo "unable to determine package version" >&2; exit 1; }

ARCH="$(uname -m)"
case "${ARCH}" in
  x86_64) ARCH_TAG="amd64" ;;
  aarch64|arm64) ARCH_TAG="arm64" ;;
  *) ARCH_TAG="${ARCH}" ;;
esac

# Runtime OS of the produced binary (a Windows dev host builds a Windows
# binary even though the production target is Linux).
OS_TAG="linux"
BINARY_SUFFIX=""
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) OS_TAG="windows"; BINARY_SUFFIX=".exe" ;;
  Darwin) OS_TAG="macos" ;;
esac

STAGING="$(mktemp -d)/${PACKAGE_ID}-${REQUESTED_VERSION}-${OS_TAG}-${ARCH_TAG}"
mkdir -p "${STAGING}/bin" "${STAGING}/config/env"
if [[ "${OS_TAG}" == "linux" ]]; then
  mkdir -p "${STAGING}/install"
fi

log "assembling ${STAGING}"
cp "${BINARY_SRC}" "${STAGING}/bin/${BINARY_NAME}${BINARY_SUFFIX}"
strip "${STAGING}/bin/${BINARY_NAME}${BINARY_SUFFIX}" 2>/dev/null || log "strip unavailable; shipping unstripped binary"
chmod 0755 "${STAGING}/bin/${BINARY_NAME}${BINARY_SUFFIX}"

if [[ "${OS_TAG}" != "linux" ]]; then
  log "non-linux dev-host build: shipping binary + config + manifest only (the Ubuntu installer belongs to linux artifacts)"
fi
if [[ "${OS_TAG}" == "linux" ]]; then
cat > "${STAGING}/install/sdkwork-payment.service" <<UNIT
[Unit]
Description=SDKWork Payment standalone API gateway
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
User=sdkwork
Group=sdkwork
EnvironmentFile=-/etc/sdkwork/payment/payment.env
ExecStart=/opt/sdkwork/payment/bin/${BINARY_NAME}
WorkingDirectory=/opt/sdkwork/payment
Restart=on-failure
RestartSec=5
# Hardening: the gateway needs only its own tree, the env file, and egress.
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/opt/sdkwork/payment
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
UNIT
fi

if [[ "${OS_TAG}" == "linux" ]]; then
cat > "${STAGING}/install/install-ubuntu.sh" <<'INSTALLER'
#!/usr/bin/env bash
# install-ubuntu.sh — idempotent installer for the sdkwork-payment server
# tarball (PACKAGING_SPEC.md §5.2, MODULE_BIN_SPEC.md §4.5). Must run as root
# on Ubuntu/Debian. It only lays out files and registers the service; starting
# the service is the deploy channel's job (sdkwork_service_enable_now).
set -euo pipefail

STAGING_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ROOT="/opt/sdkwork/payment"
ENV_FILE="/etc/sdkwork/payment/payment.env"
UNIT_FILE="/etc/systemd/system/sdkwork-payment.service"
BINARY_NAME="sdkwork-api-payment-standalone-gateway"

[[ "$(id -u)" -eq 0 ]] || { echo "installer must run as root" >&2; exit 77; }

# Architecture/linking gate: run the shipped binary before touching the host.
"${STAGING_DIR}/bin/${BINARY_NAME}" --version

id -u sdkwork >/dev/null 2>&1 || useradd --system --home-dir "${APP_ROOT}" --shell /usr/sbin/nologin sdkwork

install -d -m 0755 -o sdkwork -g sdkwork "${APP_ROOT}/bin" "${APP_ROOT}/config/env" "${APP_ROOT}/data" "${APP_ROOT}/log"
install -d -m 0750 -o root -g sdkwork /etc/sdkwork/payment
install -d -m 0755 /opt/deploy/sdkwork-payment/packages

install -m 0755 "${STAGING_DIR}/bin/${BINARY_NAME}" "${APP_ROOT}/bin/${BINARY_NAME}"
install -m 0644 "${STAGING_DIR}/install/sdkwork-payment.service" "${UNIT_FILE}"

if [[ ! -f "${ENV_FILE}" ]]; then
  install -m 0640 -o root -g sdkwork "${STAGING_DIR}/config/env/payment.env.example" "${ENV_FILE}"
  echo "[install-ubuntu] seeded ${ENV_FILE} from the packaged template; fill in real values before first start"
fi

systemctl daemon-reload
echo "[install-ubuntu] installed sdkwork-payment ${APP_ROOT} (unit: ${UNIT_FILE})"
INSTALLER
if [[ "${OS_TAG}" == "linux" ]]; then
chmod 0755 "${STAGING}/install/install-ubuntu.sh"
fi
fi

# Environment template: keys only, no values baked from the build host
# (PACKAGING_SPEC.md: artifacts carry no secrets and no environment-baked config).
cat > "${STAGING}/config/env/payment.env.example" <<'ENVTEMPLATE'
# sdkwork-payment gateway environment (installed to /etc/sdkwork/payment/payment.env)
SDKWORK_ENVIRONMENT=production
SDKWORK_DATABASE_ENGINE=postgresql
SDKWORK_DATABASE_HOST=127.0.0.1
SDKWORK_DATABASE_PORT=5432
SDKWORK_DATABASE_NAME=sdkwork_payment
SDKWORK_DATABASE_USERNAME=change_me
SDKWORK_DATABASE_PASSWORD=change_me
SDKWORK_DATABASE_MAX_CONNECTIONS=20
# Ingress bind; topology key wins over PAYMENT_API_BIND.
SDKWORK_PAYMENT_APPLICATION_PUBLIC_INGRESS_BIND=127.0.0.1:3900
SDKWORK_CORS_ALLOWED_ORIGINS=https://admin.example.com
# Provider credential master key (REQUIRED in production; absolute path outside any source checkout).
SDKWORK_PAYMENT_CREDENTIAL_MASTER_KEY_FILE=/etc/sdkwork/payment/payment-credential-master.secret
# PSP callback base for notify URLs when no notify domain row is configured.
# ORDER_PAYMENT_WEBHOOK_BASE_URL=https://api.example.com
# OTLP span export (optional; omitted = stdout tracing only).
# OTEL_EXPORTER_OTLP_ENDPOINT=http://otel-collector:4318
# OTEL_SERVICE_NAME=sdkwork-payment-standalone
# Compensation worker (defaults on at a 60s cadence).
SDKWORK_PAYMENT_COMPENSATION_ENABLED=1
ENVTEMPLATE

log "generating install-manifest.json"
MANIFEST="${STAGING}/install-manifest.json"
{
  printf '{\n'
  printf '  "packageId": "%s",\n' "${PACKAGE_ID}"
  printf '  "version": "%s",\n' "${REQUESTED_VERSION}"
  printf '  "runtimeTarget": "linux",\n'
  printf '  "architecture": "%s",\n' "${ARCH_TAG}"
  printf '  "format": "tar.gz",\n'
  printf '  "files": [\n'
  first=1
  (cd "${STAGING}" && find . -type f ! -name install-manifest.json | sed 's|^\./||' | LC_ALL=C sort) | while read -r file; do
    size=$(wc -c < "${STAGING}/${file}")
    digest=$(sha256sum "${STAGING}/${file}" | awk '{print $1}')
    [[ ${first} -eq 1 ]] || printf ',\n'
    printf '    {"path": "%s", "size": %s, "sha256": "%s"}' "${file}" "${size}" "${digest}"
    first=0
  done
  printf '\n  ]\n}\n'
} > "${MANIFEST}"

mkdir -p "${OUT_DIR}"
ARTIFACT_BASE="${PACKAGE_ID}-${REQUESTED_VERSION}-${OS_TAG}-${ARCH_TAG}"
ARTIFACT="${OUT_DIR}/${ARTIFACT_BASE}.tar.gz"
log "writing ${ARTIFACT}"
tar -czf "${ARTIFACT}" -C "$(dirname "${STAGING}")" "$(basename "${STAGING}")"
ARTIFACT_DIGEST=$(sha256sum "${ARTIFACT}" | awk '{print $1}')
printf '%s  %s\n' "${ARTIFACT_DIGEST}" "${ARTIFACT_BASE}.tar.gz" > "${ARTIFACT}.sha256"

log "packaged ${ARTIFACT} (sha256 ${ARTIFACT_DIGEST})"
