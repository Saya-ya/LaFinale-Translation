//! Local working directories. Inputs are reported but never read or patched here.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

pub const ORIGINALS: &str = "originales";
pub const WORK: &str = "work";
pub const TRANSLATIONS: &str = "work/translations";
pub const OUTPUT: &str = "output";

pub const DIRECTORIES: [&str; 4] = [ORIGINALS, WORK, TRANSLATIONS, OUTPUT];

#[derive(Debug)]
pub enum WorkspaceError {
    Io(io::Error),
    NotDirectory(PathBuf),
    Symlink(PathBuf),
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "Error al preparar las carpetas: {error}"),
            Self::NotDirectory(path) => write!(f, "Se esperaba una carpeta: {}", path.display()),
            Self::Symlink(path) => write!(
                f,
                "No se admite un enlace como carpeta de trabajo: {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for WorkspaceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug)]
pub struct Workspace {
    root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceStatus {
    pub directories: [bool; 4],
    pub has_script_pak: bool,
    pub has_exe: bool,
}

impl Workspace {
    /// Creates missing directories, but never changes the contents of existing
    /// directories or input files. A non-directory or symlink is an error.
    pub fn initialize(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let root = fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(WorkspaceError::NotDirectory(root));
        }
        for relative in DIRECTORIES {
            let path = root.join(relative);
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(WorkspaceError::Symlink(path));
                }
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(WorkspaceError::NotDirectory(path));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir(&path)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(Self { root })
    }

    pub fn status(&self) -> WorkspaceStatus {
        WorkspaceStatus {
            directories: DIRECTORIES.map(|relative| is_directory(&self.root.join(relative))),
            has_script_pak: is_regular_file(&self.root.join(ORIGINALS).join("Script.pak")),
            has_exe: is_regular_file(&self.root.join(ORIGINALS).join("SolfegeLF.exe")),
        }
    }
}

fn is_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_empty_workspace_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        assert_eq!(workspace.status().directories, [true; 4]);
        assert!(!workspace.status().has_script_pak);
        assert!(!workspace.status().has_exe);

        let original = dir.path().join(ORIGINALS).join("Script.pak");
        fs::write(&original, b"untouched").unwrap();
        fs::write(dir.path().join(ORIGINALS).join("SolfegeLF.exe"), b"exe").unwrap();
        Workspace::initialize(dir.path()).unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"untouched");
        assert!(workspace.status().has_script_pak);
        assert!(workspace.status().has_exe);
    }

    #[test]
    fn refuses_to_replace_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(ORIGINALS), b"user file").unwrap();
        let error = Workspace::initialize(dir.path()).unwrap_err();
        assert!(matches!(error, WorkspaceError::NotDirectory(_)));
        assert_eq!(fs::read(dir.path().join(ORIGINALS)).unwrap(), b"user file");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_for_work_directory() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join(WORK)).unwrap();
        let error = Workspace::initialize(dir.path()).unwrap_err();
        assert!(matches!(error, WorkspaceError::Symlink(_)));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_input_is_not_reported_as_a_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        let elsewhere = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(
            elsewhere.path(),
            dir.path().join(ORIGINALS).join("Script.pak"),
        )
        .unwrap();
        assert!(!workspace.status().has_script_pak);
    }
}
