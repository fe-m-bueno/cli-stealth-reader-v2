#!/usr/bin/env bash

set -Eeuo pipefail

: "${VERSION:?VERSION is required}"
: "${TARGET:?TARGET is required}"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "${script_dir}/.." && pwd)"
output_dir="${OUTPUT_DIR:-dist}"
binary="${repo_root}/target/${TARGET}/release/stealth-reader"
archive_name="stealth-reader-${TARGET}.tar.gz"
staging_name="stealth-reader-${VERSION}-${TARGET}"
if [[ "${output_dir}" != /* ]]; then
  output_dir="${repo_root}/${output_dir}"
fi
archive_path="${output_dir}/${archive_name}"

if [[ ! -x "${binary}" ]]; then
  printf 'Release binary not found or is not executable: %s\n' "${binary}" >&2
  exit 1
fi

mkdir -p "${output_dir}"
staging_dir="$(mktemp -d)"
staging_package_dir="${staging_dir}/${staging_name}"
trap 'rm -rf -- "${staging_dir}"' EXIT

mkdir -p "${staging_package_dir}"
cp "${binary}" "${staging_package_dir}/stealth-reader"
cp "${repo_root}/README.md" "${staging_package_dir}/README.md"

if [[ -f "${repo_root}/LICENSE" ]]; then
  cp "${repo_root}/LICENSE" "${staging_package_dir}/LICENSE"
fi

tar -C "${staging_dir}" -czf "${archive_path}" "${staging_name}"

if command -v sha256sum >/dev/null 2>&1; then
  (cd "${output_dir}" && sha256sum "${archive_name}") > "${archive_path}.sha256"
else
  (cd "${output_dir}" && shasum -a 256 "${archive_name}") > "${archive_path}.sha256"
fi

printf 'Packaged %s\n' "${archive_path}"
