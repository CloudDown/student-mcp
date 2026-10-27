use russh::keys::ssh_key::{private::Ed25519Keypair, PrivateKey};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) struct Identity {
    pub key: PrivateKey,
    pub origin: String,
    pub host_file: PathBuf,
}

impl Identity {
    pub fn load(config_path: &Path) -> Result<Self, String> {
        let path = config_path.with_extension("srvus-key");
        let seed = read_or_create(&path, || {
            let mut seed = vec![0; 32];
            getrandom::getrandom(&mut seed).map_err(|_| io::Error::other("OS randomness unavailable"))?;
            Ok(seed)
        }).map_err(|_| "Cannot read or save the public-link identity. Check your app configuration folder permissions; existing keys were not replaced.".to_string())?;
        let seed: [u8; 32] = seed.try_into().map_err(|_| "The saved public-link identity is invalid. It was not replaced; restore it from your backup to keep the same link.".to_string())?;
        let key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
        // srv.us endpointURLs: SHA256(SSH public key wire bytes || NUL || "1").
        // Derive locally instead of trusting an arbitrary URL in server output.
        let mut hash = Sha256::new();
        hash.update(
            key.public_key()
                .to_bytes()
                .map_err(|_| "Cannot encode the public-link identity.".to_string())?,
        );
        hash.update(b"\0");
        hash.update(b"1");
        let digest = hash.finalize();
        let name = data_encoding::BASE32_NOPAD
            .encode(&digest[..16])
            .to_ascii_lowercase();
        Ok(Self {
            key,
            origin: format!("https://{name}.srv.us"),
            host_file: config_path.with_extension("srvus-host"),
        })
    }
}

pub(super) fn verify_host(path: &Path, offered: &[u8]) -> Result<(), String> {
    // Trust on first use, then refuse changes. Never overwrite an existing pin.
    let saved = read_or_create(path, || Ok(offered.to_vec()))
        .map_err(|_| "Cannot read or save the srv.us server identity. Check your configuration folder permissions.".to_string())?;
    if saved != offered {
        return Err("The srv.us server identity changed. Public access was stopped for safety. Do not remove the saved host key without verifying the change with srv.us.".into());
    }
    Ok(())
}

fn read_or_create(
    path: &Path,
    generate: impl FnOnce() -> io::Result<Vec<u8>>,
) -> io::Result<Vec<u8>> {
    match fs::symlink_metadata(path) {
        Ok(_) => read_existing(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent)?;
            }
            let bytes = generate()?;
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(path) {
                Ok(mut file) => {
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    Ok(bytes)
                }
                // Another instance won creation. Never overwrite its identity.
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => read_existing(path),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn read_existing(path: &Path) -> io::Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::other("Identity must be a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    fs::read(path)
}
