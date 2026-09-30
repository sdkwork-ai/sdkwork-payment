#!/usr/bin/env bash
# module.sh — sdkwork-payment bin/ wiring (MODULE_BIN_SPEC.md §3).
# Scaffolded by sdkwork-specs/tools/scaffold-module-bin.mjs; replace the
# unwired hooks with the repository's canonical commands as they land.
# Every shared primitive comes from sdkwork-specs/bin/lib/sdkwork-common.sh.

SDKWORK_MODULE_ID="sdkwork-payment"
SDKWORK_IMAGE_NAME="sdkwork-payment-standalone"
SDKWORK_APP_TYPES="server,pc"
SDKWORK_MODULE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPO_ROOT="${SDKWORK_MODULE_ROOT}"

# Operations wiring (OPERATIONS_SPEC.md): compose service carrying the health
# probe and its path; adjust to the module's compose file when it lands.
SDKWORK_PRIMARY_SERVICE="app"
SDKWORK_HEALTH_PATH="/healthz"
SDKWORK_CONFIG_ENV_SUBDIR="env"

# ----------------------------------------------------------------------------
# Container image (docker-image.sh build)
# ----------------------------------------------------------------------------
sdkwork_image_build() {
  local ref="$1" tag="$2"
  # Canonical container build: self-contained multi-stage Dockerfile at deployments/docker/Dockerfile
  # (build context = repository root). Wires MODULE_BIN_SPEC.md §4.1; the
  # shared entrypoint asserts the ${ref} image exists after this returns.
  sdkwork_local_run docker build -f "deployments/docker/Dockerfile" -t "${ref}" .
}

# ----------------------------------------------------------------------------
# Application build (apps-build.sh)
# ----------------------------------------------------------------------------
sdkwork_build_app() {
  local app_type="$1" environment="$2" profile="$3"
  case "${app_type}" in
    server)
      sdkwork_local_run cargo build --release ;;
    pc)
      # Canonical browser bundle build (FRONTEND_CODE_SPEC dist layout).
      sdkwork_local_run pnpm --dir apps/sdkwork-payment-pc build ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" \
        "app type '${app_type}' has no wired build for sdkwork-payment; extend sdkwork_build_app with the repository's canonical runner (declared: ${SDKWORK_APP_TYPES})" ;;
  esac
}

# ----------------------------------------------------------------------------
# Application packaging (apps-package.sh)
# ----------------------------------------------------------------------------
sdkwork_package_app() {
  local app_type="$1" environment="$2" profile="$3" out="$4"
  case "${app_type}" in
    server)
      # Canonical packager (PACKAGING_SPEC.md §5.2/§6): release tarball +
      # install/install-ubuntu.sh + generated install-manifest.json + .sha256.
      # Environment/profile are recorded in the artifact name context only —
      # artifacts carry no environment-baked configuration.
      sdkwork_local_run bash "${SDKWORK_MODULE_ROOT}/scripts/package-server.sh" --out "${out}"
      ;;
    pc)
      sdkwork_die "${SDKWORK_BIN_E_STATE}" \
        "sdkwork-payment's pc surface is a host-mounted capability library, not a standalone published bundle; no web-static packager is declared. Ship it through the federated host application or extend sdkwork_package_app with the repository's web packager (MODULE_BIN_SPEC.md §4.4)"
      ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" \
        "app type '${app_type}' has no wired packager for sdkwork-payment (declared: ${SDKWORK_APP_TYPES})" ;;
  esac
}

# ----------------------------------------------------------------------------
# Native installer packaging (apps-pkg-installer.sh, MODULE_BIN_SPEC.md §4.9)
# ----------------------------------------------------------------------------
sdkwork_installer_app() {
  local app_type="$1" platform="$2" environment="$3" profile="$4" out="$5" arch="$6" format="$7"
  sdkwork_die "${SDKWORK_BIN_E_STATE}" \
    "sdkwork-payment has no native installer builder wired yet; implement sdkwork_installer_app against the repository's installer commands (MODULE_BIN_SPEC.md §4.9; platforms: windows|linux|macos|android|ios)"
}

# ----------------------------------------------------------------------------
# Application deployment (apps-deploy.sh)
# ----------------------------------------------------------------------------
sdkwork_deploy_app() {
  local app_type="$1" action="$2" environment="$3" profile="$4" host="$5"
  case "${app_type}" in
    server) sdkwork_deploy_server "${action}" "${environment}" "${profile}" "${host}" ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_ENV}" \
        "app type '${app_type}' has no wired deployment channel for sdkwork-payment (declared: ${SDKWORK_APP_TYPES})" ;;
  esac
}

# §4.5 server host-native channel: push the packaged tarball to
# /opt/deploy/<module-id>/packages, install it with the package's own
# installer (root account; wrappers inject no sudo), enable+start the unit,
# then verify is-active and the health endpoint.
SDKWORK_DEPLOY_PACKAGE_DIR="/opt/deploy/sdkwork-payment/packages"
SDKWORK_DEPLOY_UNIT="sdkwork-payment.service"
# Health probe target on the deployed host; override when the gateway binds
# elsewhere (topology key SDKWORK_PAYMENT_APPLICATION_PUBLIC_INGRESS_BIND).
SDKWORK_DEPLOY_HEALTH_URL="${SDKWORK_DEPLOY_HEALTH_URL:-http://127.0.0.1:18094/healthz}"

sdkwork_deploy_server() {
  local action="$1" environment="$2" profile="$3" host="$4"
  case "${action}" in
    rollback)
      sdkwork_die "${SDKWORK_BIN_E_STATE}" \
        "rollback is not tracked by the host-native package channel: package the previous version and run 'apps-deploy.sh server install ${environment}' against it (MODULE_BIN_SPEC.md §4.5)" ;;
    status)
      sdkwork_remote "${host}" systemctl is-active "${SDKWORK_DEPLOY_UNIT}" || true
      sdkwork_health_probe "${host}" "${SDKWORK_DEPLOY_HEALTH_URL}" \
        && sdkwork_log "health probe ok: ${SDKWORK_DEPLOY_HEALTH_URL}" \
        || sdkwork_log "health probe FAILED: ${SDKWORK_DEPLOY_HEALTH_URL}"
      ;;
    install|upgrade)
      local packages_dir="${REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}/target/bin-packages"
      [[ -d "${packages_dir}" ]] || sdkwork_die "${SDKWORK_BIN_E_STATE}" \
        "no packaged artifacts at ${packages_dir}; run 'apps-package.sh server ${environment}' first"
      # Latest artifact wins (packaging writes one tarball + .sha256 per run).
      local artifact
      artifact="$(ls -1t "${packages_dir}"/sdkwork-payment-server-*.tar.gz 2>/dev/null | head -1)" \
        || sdkwork_die "${SDKWORK_BIN_E_STATE}" "no sdkwork-payment-server-*.tar.gz in ${packages_dir}"
      [[ -n "${artifact}" ]] || sdkwork_die "${SDKWORK_BIN_E_STATE}" "no packaged artifact found in ${packages_dir}"
      sdkwork_log "deploying $(basename "${artifact}") -> ${host}:${SDKWORK_DEPLOY_PACKAGE_DIR}"
      sdkwork_remote_parse "${host}"
      if [[ "${SDKWORK_REMOTE_KIND}" == "ssh" ]]; then
        sdkwork_run ssh -p "${SDKWORK_SSH_PORT}" "${SDKWORK_SSH_USER:+${SDKWORK_SSH_USER}@}${SDKWORK_SSH_HOST}" \
          "mkdir -p '${SDKWORK_DEPLOY_PACKAGE_DIR}'"
        sdkwork_run scp -P "${SDKWORK_SSH_PORT}" "${artifact}" "${SDKWORK_SSH_USER:+${SDKWORK_SSH_USER}@}${SDKWORK_SSH_HOST}:${SDKWORK_DEPLOY_PACKAGE_DIR}/"
        sdkwork_remote "${host}" bash -lc "'
          set -euo pipefail
          pkg_dir="${SDKWORK_DEPLOY_PACKAGE_DIR}"
          tarball="\${pkg_dir}/$(basename "${artifact}")"
          staging="/opt/deploy/sdkwork-payment/staging"
          rm -rf "${staging}"
          mkdir -p "${staging}"
          tar -xzf "\${tarball}" -C "\${staging}"
          (cd "\${staging}"/* && ./install/install-ubuntu.sh)
        '"
      else
        # wsl/local target: run the same steps directly.
        sdkwork_remote "${host}" bash -lc "'
          set -euo pipefail
          pkg_dir="${SDKWORK_DEPLOY_PACKAGE_DIR}"
          mkdir -p "\${pkg_dir}"
          cp "${artifact}" "\${pkg_dir}/"
          staging="/opt/deploy/sdkwork-payment/staging"
          rm -rf "\${staging}"
          mkdir -p "\${staging}"
          tar -xzf "\${pkg_dir}/$(basename "${artifact}")" -C "\${staging}"
          (cd "\${staging}"/* && ./install/install-ubuntu.sh)
        '"
      fi
      sdkwork_service_enable_now "${host}" "${SDKWORK_DEPLOY_UNIT}"
      sdkwork_remote "${host}" systemctl is-active "${SDKWORK_DEPLOY_UNIT}"
      sdkwork_health_probe "${host}" "${SDKWORK_DEPLOY_HEALTH_URL}"
      sdkwork_log "deploy complete: ${action} on ${host} (unit ${SDKWORK_DEPLOY_UNIT} active, health ok)"
      ;;
    *)
      sdkwork_die "${SDKWORK_BIN_E_USAGE}" "unknown deploy action '${action}' (use install|upgrade|rollback|status)" ;;
  esac
}
