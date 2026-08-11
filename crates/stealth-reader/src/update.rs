//! Self-update support for the installed release binary.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const RELEASE_URL: &str =
    "https://github.com/fe-m-bueno/cli-stealth-reader-v2/releases/latest/download";
const BINARY_NAME: &str = "stealth-reader";

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let target = target_for_host(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or_else(|| io::Error::other("plataforma não suportada pela release"))?;
    let current_executable = std::env::current_exe()?;
    let temporary_dir = temporary_directory()?;

    let result = update_from_release(target, &current_executable, &temporary_dir);
    let cleanup_result = fs::remove_dir_all(&temporary_dir);

    result?;
    cleanup_result?;
    Ok(())
}

fn update_from_release(
    target: &str,
    current_executable: &Path,
    temporary_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let archive_name = format!("{BINARY_NAME}-{target}.tar.gz");
    let archive_path = temporary_dir.join(&archive_name);
    let checksum_path = temporary_dir.join(format!("{archive_name}.sha256"));
    let archive_url = format!("{RELEASE_URL}/{archive_name}");
    let checksum_url = format!("{RELEASE_URL}/{}.sha256", archive_name);

    println!("stealth-reader: baixando a atualização mais recente");
    download(&archive_url, &archive_path)?;
    download(&checksum_url, &checksum_path)?;

    println!("stealth-reader: verificando o checksum SHA-256");
    verify_checksum(&archive_path, &checksum_path)?;

    println!("stealth-reader: extraindo o novo binário");
    extract_archive(&archive_path, temporary_dir)?;
    let new_executable = find_binary(temporary_dir)?
        .ok_or_else(|| io::Error::other("o release não contém o binário stealth-reader"))?;

    println!("stealth-reader: substituindo o executável atual");
    replace_executable(&new_executable, current_executable)?;
    println!("stealth-reader: atualização concluída");
    Ok(())
}

fn target_for_host(operating_system: &str, architecture: &str) -> Option<&'static str> {
    match (operating_system, architecture) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}

fn temporary_directory() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "{BINARY_NAME}-update-{}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir(&directory)?;
    Ok(directory)
}

fn download(url: &str, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--retry",
            "3",
        ])
        .arg("--output")
        .arg(output)
        .arg(url)
        .status()
        .map_err(|error| io::Error::other(format!("não foi possível executar curl: {error}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("curl terminou com {status}")).into())
    }
}

fn verify_checksum(
    archive_path: &Path,
    checksum_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let checksum_contents = fs::read_to_string(checksum_path)?;
    let expected = checksum_contents
        .split_whitespace()
        .next()
        .ok_or_else(|| io::Error::other("o arquivo de checksum está vazio"))?;
    let archive = fs::read(archive_path)?;
    let actual = hex_digest(Sha256::digest(archive));

    if expected == actual {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "checksum inválido: esperado {expected}, obtido {actual}"
        ))
        .into())
    }
}

fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = digest.as_ref();
    let mut result = String::with_capacity(digest.len() * 2);

    for byte in digest {
        result.push(HEX[usize::from(*byte >> 4)] as char);
        result.push(HEX[usize::from(*byte & 0x0f)] as char);
    }
    result
}

fn extract_archive(
    archive_path: &Path,
    output_directory: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("tar")
        .args(["-xzf"])
        .arg(archive_path)
        .args(["-C"])
        .arg(output_directory)
        .status()
        .map_err(|error| io::Error::other(format!("não foi possível executar tar: {error}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("tar terminou com {status}")).into())
    }
}

fn find_binary(directory: &Path) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.file_name() == Some(OsStr::new(BINARY_NAME)) && path.is_file() {
            return Ok(Some(path));
        }
        if path.is_dir()
            && let Some(binary) = find_binary(&path)?
        {
            return Ok(Some(binary));
        }
    }
    Ok(None)
}

fn replace_executable(
    new_executable: &Path,
    current_executable: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let file_name = current_executable
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| io::Error::other("não foi possível identificar o executável atual"))?;
    let replacement =
        current_executable.with_file_name(format!(".{file_name}.update-{}", std::process::id()));

    fs::copy(new_executable, &replacement)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&replacement)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&replacement, permissions)?;
    }

    if let Err(error) = fs::rename(&replacement, current_executable) {
        let _ = fs::remove_file(&replacement);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_supported_hosts_to_release_targets() {
        assert_eq!(
            target_for_host("linux", "x86_64"),
            Some("x86_64-unknown-linux-gnu")
        );
        assert_eq!(
            target_for_host("macos", "x86_64"),
            Some("x86_64-apple-darwin")
        );
        assert_eq!(
            target_for_host("macos", "aarch64"),
            Some("aarch64-apple-darwin")
        );
    }

    #[test]
    fn rejects_hosts_without_a_published_release() {
        assert_eq!(target_for_host("linux", "aarch64"), None);
        assert_eq!(target_for_host("windows", "x86_64"), None);
    }

    #[test]
    fn verifies_a_matching_archive_checksum() {
        let directory = temporary_directory().expect("temporary directory");
        let archive = directory.join("archive");
        let checksum = directory.join("archive.sha256");
        fs::write(&archive, b"release bytes").expect("archive");
        let digest = hex_digest(Sha256::digest(b"release bytes"));
        fs::write(&checksum, format!("{digest}  archive\n")).expect("checksum");

        assert!(verify_checksum(&archive, &checksum).is_ok());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn rejects_a_mismatched_archive_checksum() {
        let directory = temporary_directory().expect("temporary directory");
        let archive = directory.join("archive");
        let checksum = directory.join("archive.sha256");
        fs::write(&archive, b"release bytes").expect("archive");
        fs::write(&checksum, "000000  archive\n").expect("checksum");

        assert!(verify_checksum(&archive, &checksum).is_err());
        fs::remove_dir_all(directory).expect("cleanup");
    }
}
