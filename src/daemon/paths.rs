use std::env;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use super::{error, Result};

#[derive(Clone)]
pub struct Paths {
    pub state: PathBuf,
    run: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let state_override = env::var_os("TAKT_STATE_DIR").map(PathBuf::from);
        let run_override = env::var_os("TAKT_RUNTIME_DIR").map(PathBuf::from);
        let runtime =
            env::var_os("XDG_RUNTIME_DIR").map(|path| PathBuf::from(path).join("takt-rs"));
        let state = match state_override.as_ref() {
            Some(path) => path.clone(),
            None => env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
                })
                .ok_or_else(|| error("set TAKT_STATE_DIR or HOME"))?
                .join("takt-rs"),
        };
        let run = run_override
            .or_else(|| state_override.is_none().then_some(runtime).flatten())
            .unwrap_or_else(|| state.join("run"));
        if !state.is_absolute() || !run.is_absolute() {
            return Err(error("state and runtime directories must be absolute"));
        }
        Ok(Self { state, run })
    }

    #[cfg(test)]
    pub fn test(root: &Path) -> Self {
        Self {
            state: root.join("state"),
            run: root.join("run"),
        }
    }

    pub fn socket(&self) -> PathBuf {
        self.run.join("daemon.sock")
    }

    pub fn database(&self) -> PathBuf {
        self.state.join("takt.sqlite3")
    }

    pub fn connect(&self) -> Result<UnixStream> {
        validate_dir(&self.run)?;
        let stream = UnixStream::connect(self.socket())?;
        check_peer(&stream)?;
        Ok(stream)
    }

    pub fn bind(&self) -> Result<BoundSocket> {
        private_dir(&self.state)?;
        private_dir(&self.run)?;
        let _state_lock = lock(&self.state.join("writer.lock"))?;
        let _socket_lock = lock(&self.run.join("socket.lock"))?;
        let socket = self.socket();
        match fs::symlink_metadata(&socket) {
            Ok(meta) if meta.file_type().is_socket() && meta.uid() == uid() => {
                fs::remove_file(&socket)?
            }
            Ok(_) => {
                return Err(error(
                    "daemon socket path is occupied by an unexpected file",
                ))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        let listener = UnixListener::bind(&socket)?;
        let bound = BoundSocket {
            listener,
            socket,
            _state_lock,
            _socket_lock,
        };
        fs::set_permissions(&bound.socket, fs::Permissions::from_mode(0o600))?;
        bound.listener.set_nonblocking(true)?;
        Ok(bound)
    }
}

pub struct BoundSocket {
    pub listener: UnixListener,
    socket: PathBuf,
    _state_lock: File,
    _socket_lock: File,
}

impl Drop for BoundSocket {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
    }
}

fn uid() -> u32 {
    // SAFETY: geteuid has no inputs or memory requirements.
    unsafe { libc::geteuid() }
}

pub fn check_peer(stream: &UnixStream) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    let mut cred = std::mem::MaybeUninit::<libc::ucred>::uninit();
    let mut size = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: cred points to a writable ucred-sized buffer and size is initialized.
    let status = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            cred.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    if size as usize != std::mem::size_of::<libc::ucred>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unexpected peer credential size",
        ));
    }
    // SAFETY: getsockopt succeeded and reported a complete ucred.
    if unsafe { cred.assume_init() }.uid != uid() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "daemon peer UID mismatch",
        ));
    }
    Ok(())
}

fn private_dir(path: &Path) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    validate_dir(path)?;
    Ok(())
}

fn validate_dir(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.uid() != uid() || meta.mode() & 0o777 != 0o700 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Takt directory must be owned by the current UID and have mode 0700",
        ));
    }
    Ok(())
}

fn lock(path: &Path) -> Result<File> {
    let guard = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    match guard.try_lock() {
        Ok(()) => Ok(guard),
        Err(std::fs::TryLockError::WouldBlock) => {
            Err(error("a Takt daemon already owns this state or socket"))
        }
        Err(std::fs::TryLockError::Error(err)) => Err(err.into()),
    }
}
