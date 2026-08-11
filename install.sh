#!/usr/bin/env bash

set -eu

readonly REPOSITORY="fe-m-bueno/cli-stealth-reader-v2"
readonly RELEASE_URL="https://github.com/${REPOSITORY}/releases/latest/download"
readonly INSTALL_DIR="${STEALTH_READER_INSTALL_DIR:-${XDG_BIN_HOME:-${HOME}/.local/bin}}"

# cli-spinners@dots — interval: 80ms
spinner_frame() {
  case "$1" in
    0) printf '⠋' ;;
    1) printf '⠙' ;;
    2) printf '⠹' ;;
    3) printf '⠸' ;;
    4) printf '⠼' ;;
    5) printf '⠴' ;;
    6) printf '⠦' ;;
    7) printf '⠧' ;;
    8) printf '⠇' ;;
    *) printf '⠏' ;;
  esac
}

die() {
  printf '\n✗ %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "comando obrigatório não encontrado: $1"
}

cleanup() {
  if [ -n "${temporary_dir:-}" ] && [ -d "${temporary_dir}" ]; then
    rm -rf -- "${temporary_dir}"
  fi
}

run_step() {
  label="$1"
  shift
  log_file="${temporary_dir}/step.log"

  if [ -t 1 ] && [ "${TERM:-dumb}" != "dumb" ]; then
    : > "${log_file}"
    "$@" >"${log_file}" 2>&1 &
    command_pid=$!
    frame=0

    while kill -0 "${command_pid}" 2>/dev/null; do
      printf '\r\033[2K  %s %s' "$(spinner_frame "${frame}")" "${label}"
      frame=$(( (frame + 1) % 10 ))
      sleep 0.08
    done

    status=0
    wait "${command_pid}" || status=$?
    if [ "${status}" -ne 0 ]; then
      printf '\r\033[2K  ✗ %s\n' "${label}" >&2
      cat "${log_file}" >&2
      return "${status}"
    fi

    printf '\r\033[2K  ✓ %s\n' "${label}"
    return 0
  fi

  "$@"
}

target_for_host() {
  operating_system="$(uname -s)"
  architecture="$(uname -m)"

  case "${operating_system}:${architecture}" in
    Linux:x86_64|Linux:amd64)
      printf 'x86_64-unknown-linux-gnu'
      ;;
    Darwin:x86_64)
      printf 'x86_64-apple-darwin'
      ;;
    Darwin:arm64|Darwin:aarch64)
      printf 'aarch64-apple-darwin'
      ;;
    *)
      die "plataforma não suportada: ${operating_system} ${architecture}"
      ;;
  esac
}

verify_checksum() {
  archive="$1"
  checksum_file="$2"
  expected="$(sed 's/[[:space:]].*$//' "${checksum_file}")"

  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "${archive}" | sed 's/[[:space:]].*$//')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "${archive}" | sed 's/[[:space:]].*$//')"
  else
    die "não encontrei sha256sum nem shasum para verificar o download"
  fi

  [ -n "${expected}" ] && [ "${expected}" = "${actual}" ] || {
    printf 'esperado: %s\nobtido:   %s\n' "${expected}" "${actual}" >&2
    die "a verificação SHA-256 falhou"
  }
}

require_command curl
require_command tar
require_command mktemp
require_command sed

target="$(target_for_host)"
archive_name="stealth-reader-${target}.tar.gz"
checksum_name="${archive_name}.sha256"
temporary_dir="$(mktemp -d)"
trap cleanup 0

printf '\n'
printf '  ╭────────────────────────────────────╮\n'
printf '  │  stealth-reader · instalação         │\n'
printf '  ╰────────────────────────────────────╯\n'
printf '  plataforma: %s\n\n' "${target}"

run_step "baixando o release mais recente" \
  curl --fail --silent --show-error --location --retry 3 \
  --output "${temporary_dir}/${archive_name}" \
  "${RELEASE_URL}/${archive_name}"

run_step "baixando o checksum SHA-256" \
  curl --fail --silent --show-error --location --retry 3 \
  --output "${temporary_dir}/${checksum_name}" \
  "${RELEASE_URL}/${checksum_name}"

run_step "verificando a integridade do download" \
  verify_checksum "${temporary_dir}/${archive_name}" \
  "${temporary_dir}/${checksum_name}"

run_step "extraindo o binário" \
  tar -xzf "${temporary_dir}/${archive_name}" \
  -C "${temporary_dir}"

archive_root="$(tar -tzf "${temporary_dir}/${archive_name}" | sed -n '1s|/.*||p')"
[ -n "${archive_root}" ] || die "o release não contém um diretório de instalação"
binary="${temporary_dir}/${archive_root}/stealth-reader"
[ -f "${binary}" ] || die "o release não contém o binário stealth-reader"

run_step "instalando em ${INSTALL_DIR}" \
  mkdir -p "${INSTALL_DIR}"
run_step "finalizando a instalação" \
  cp "${binary}" "${INSTALL_DIR}/stealth-reader"
chmod 755 "${INSTALL_DIR}/stealth-reader"

printf '\n  ✓ instalação concluída\n'
printf '  execute: %s\n' "${INSTALL_DIR}/stealth-reader --version"

case ":${PATH:-}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    printf '\n  adicione este diretório ao PATH para chamar o comando diretamente:\n'
    printf '  export PATH="%s:$PATH"\n' "${INSTALL_DIR}"
    ;;
esac
