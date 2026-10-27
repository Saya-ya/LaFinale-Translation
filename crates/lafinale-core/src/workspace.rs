//! Local working directories. Inputs are reported but never read or patched here.

use std::{
    fmt, fs, io,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    exe_patch::{self, PatchError},
    kgo,
    pak::{self, Archive},
    pak_build,
};

pub const ORIGINALS: &str = "originales";
pub const WORK: &str = "work";
pub const TRANSLATIONS: &str = "work/translations";
pub const OUTPUT: &str = "output";

pub const DIRECTORIES: [&str; 4] = [ORIGINALS, WORK, TRANSLATIONS, OUTPUT];
pub const DIAGNOSTIC_EXE: &str = "SolfegeLF_DIAGNOSTIC.exe";

#[derive(Debug)]
pub enum PakBuildError {
    MissingInput,
    OutputExists,
    Invalid(String),
    Io(io::Error),
}

impl fmt::Display for PakBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput => write!(
                f,
                "Coloca Script.pak original y 117 JSON en las carpetas de trabajo"
            ),
            Self::OutputExists => write!(f, "Ya existe output/Script.pak; no se sobrescribe"),
            Self::Invalid(message) => write!(f, "No se generó el PAK: {message}"),
            Self::Io(error) => write!(f, "Error de entrada/salida: {error}"),
        }
    }
}
impl From<io::Error> for PakBuildError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug)]
pub struct PakBuildSummary {
    pub sha256: String,
    pub report: pak_build::BuildReport,
}

#[derive(Debug)]
pub enum ImportError {
    MissingInput,
    Occupied,
    Invalid(String),
    Io(io::Error),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput => write!(f, "Coloca el PAK original en originales/Script.pak"),
            Self::Occupied => write!(
                f,
                "work/translations/ ya contiene archivos; no se sobrescriben"
            ),
            Self::Invalid(message) => write!(f, "No se importó el PAK: {message}"),
            Self::Io(error) => write!(f, "Error de entrada/salida: {error}"),
        }
    }
}

impl From<io::Error> for ImportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub scripts: usize,
    pub strings: usize,
}

#[derive(Debug)]
pub enum BuildError {
    MissingInput,
    OutputExists,
    UnsafeInput,
    Patch(PatchError),
    Io(io::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingInput => write!(
                f,
                "Coloca el EXE NoDVD compatible en originales/SolfegeLF.exe"
            ),
            Self::OutputExists => write!(
                f,
                "Ya existe output/SolfegeLF_DIAGNOSTIC.exe; no se sobrescribe"
            ),
            Self::UnsafeInput => write!(f, "La entrada debe ser un archivo regular, no un enlace"),
            Self::Patch(error) => error.fmt(f),
            Self::Io(error) => write!(f, "Error de entrada/salida: {error}"),
        }
    }
}

impl From<io::Error> for BuildError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<PatchError> for BuildError {
    fn from(error: PatchError) -> Self {
        Self::Patch(error)
    }
}

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
    pub translation_json_count: Option<usize>,
    pub diagnostic_exe_present: bool,
    pub output_pak_present: bool,
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
            translation_json_count: count_json_files(&self.root.join(TRANSLATIONS)),
            diagnostic_exe_present: is_regular_file(&self.root.join(OUTPUT).join(DIAGNOSTIC_EXE)),
            output_pak_present: is_regular_file(&self.root.join(OUTPUT).join("Script.pak")),
        }
    }

    /// Imports the exact original PAK into *new* JSON files. No original game
    /// data is modified; all 117 scripts are validated before writing any JSON.
    pub fn import_translations(&self) -> Result<ImportSummary, ImportError> {
        let source = self.root.join(ORIGINALS).join("Script.pak");
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(ImportError::MissingInput)
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
            return Err(ImportError::Invalid(
                "se espera un archivo regular de hasta 32 MiB".into(),
            ));
        }
        let destination = self.root.join(TRANSLATIONS);
        if fs::read_dir(&destination)?.next().is_some() {
            return Err(ImportError::Occupied);
        }
        let data = fs::read(source)?;
        let hash = exe_patch::sha256(&data);
        if hash != pak::ORIGINAL_SHA256 {
            return Err(ImportError::Invalid(format!(
                "versión de PAK no reconocida (SHA-256: {hash})"
            )));
        }
        let archive =
            Archive::parse(&data).map_err(|error| ImportError::Invalid(error.to_string()))?;
        let mut staged = Vec::with_capacity(117);
        let mut strings = 0;
        for (index, entry) in archive.entries.iter().enumerate().skip(1) {
            let raw = archive
                .unpack(index)
                .map_err(|error| ImportError::Invalid(error.to_string()))?;
            let script = kgo::extract(&raw, &entry.name)
                .map_err(|error| ImportError::Invalid(format!("{}: {error}", entry.name)))?;
            strings += script.string_count;
            let json = serde_json::to_vec_pretty(&script)
                .map_err(|error| ImportError::Invalid(error.to_string()))?;
            let name = entry
                .name
                .strip_suffix(".kgo")
                .expect("nombre validado en el PAK");
            staged.push((destination.join(format!("{name}.json")), json));
        }
        // Reject unknown contents as well, not only existing JSON files.
        if fs::read_dir(&destination)?.next().is_some() {
            return Err(ImportError::Occupied);
        }
        let mut created = Vec::new();
        for (path, bytes) in &staged {
            let result = (|| -> io::Result<()> {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)?;
                created.push(path.clone());
                file.write_all(bytes)?;
                file.sync_all()
            })();
            if let Err(error) = result {
                for path in created {
                    let _ = fs::remove_file(path);
                }
                if error.kind() == io::ErrorKind::AlreadyExists {
                    return Err(ImportError::Occupied);
                }
                return Err(error.into());
            }
        }
        Ok(ImportSummary {
            scripts: staged.len(),
            strings,
        })
    }

    /// Build to a fixed new local file after validating all scripts in memory.
    /// Existing outputs and original game data are never overwritten.
    pub fn build_pak_diagnostic(&self) -> Result<PakBuildSummary, PakBuildError> {
        let source = self.root.join(ORIGINALS).join("Script.pak");
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(PakBuildError::MissingInput)
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
            return Err(PakBuildError::Invalid(
                "PAK de entrada inseguro o demasiado grande".into(),
            ));
        }
        let output_path = self.root.join(OUTPUT).join("Script.pak");
        if fs::symlink_metadata(&output_path).is_ok() {
            return Err(PakBuildError::OutputExists);
        }
        let json_dir = self.root.join(TRANSLATIONS);
        let mut docs = std::collections::HashMap::new();
        for entry in fs::read_dir(json_dir)? {
            let entry = entry?;
            if entry.path().extension() != Some(std::ffi::OsStr::new("json")) {
                continue;
            }
            if !entry.file_type()?.is_file() {
                return Err(PakBuildError::Invalid(
                    "JSON no es un archivo regular".into(),
                ));
            }
            if entry.metadata()?.len() > 8 * 1024 * 1024 {
                return Err(PakBuildError::Invalid("JSON demasiado grande".into()));
            }
            let document: kgo::Script = serde_json::from_slice(&fs::read(entry.path())?)
                .map_err(|error| PakBuildError::Invalid(format!("JSON inválido: {error}")))?;
            let expected = format!(
                "{}.json",
                document
                    .filename
                    .strip_suffix(".kgo")
                    .ok_or_else(|| PakBuildError::Invalid("filename no termina en .kgo".into()))?
            );
            if entry.file_name() != std::ffi::OsStr::new(&expected)
                || docs.insert(document.filename.clone(), document).is_some()
            {
                return Err(PakBuildError::Invalid(
                    "nombre/filename JSON no coincidente o duplicado".into(),
                ));
            }
        }
        if docs.len() != 117 {
            return Err(PakBuildError::MissingInput);
        }
        let (bytes, report) = pak_build::build(&fs::read(source)?, &docs)
            .map_err(|error| PakBuildError::Invalid(error.to_string()))?;
        let sha256 = exe_patch::sha256(&bytes);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(PakBuildError::OutputExists)
            }
            Err(error) => return Err(error.into()),
        };
        let result = output.write_all(&bytes).and_then(|_| output.sync_all());
        drop(output);
        if let Err(error) = result {
            let _ = fs::remove_file(output_path);
            return Err(PakBuildError::Io(error));
        }
        Ok(PakBuildSummary { sha256, report })
    }

    /// Reads the fixed local input and writes one new diagnostic output. No
    /// arbitrary path from HTTP is accepted; create_new prevents overwrites.
    pub fn build_exe_diagnostic(&self) -> Result<&'static str, BuildError> {
        let source = self.root.join(ORIGINALS).join("SolfegeLF.exe");
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(BuildError::MissingInput)
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.is_file() {
            return Err(BuildError::UnsafeInput);
        }
        let input = fs::read(source)?;
        let patched = exe_patch::build_diagnostic(&input)?;
        let destination = self.root.join(OUTPUT).join(DIAGNOSTIC_EXE);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(BuildError::OutputExists)
            }
            Err(error) => return Err(error.into()),
        };
        let result = output.write_all(&patched).and_then(|_| output.sync_all());
        drop(output);
        if let Err(error) = result {
            // Only remove the file created by this call, never an existing output.
            let _ = fs::remove_file(destination);
            return Err(BuildError::Io(error));
        }
        Ok(DIAGNOSTIC_EXE)
    }
}

fn is_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn count_json_files(path: &Path) -> Option<usize> {
    if !is_directory(path) {
        return None;
    }
    let mut count = 0;
    for entry in fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        if entry.file_type().ok()?.is_file()
            && entry.path().extension() == Some(std::ffi::OsStr::new("json"))
        {
            count += 1;
        }
    }
    Some(count)
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
        assert_eq!(workspace.status().translation_json_count, Some(0));

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

    #[test]
    fn counts_json_without_claiming_translations_are_valid() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        fs::write(dir.path().join(TRANSLATIONS).join("a.json"), b"not json").unwrap();
        fs::write(dir.path().join(TRANSLATIONS).join("b.txt"), b"other").unwrap();
        fs::write(dir.path().join(OUTPUT).join("Script.pak"), b"not pak").unwrap();
        assert_eq!(workspace.status().translation_json_count, Some(1));
        assert!(workspace.status().output_pak_present);
        assert!(!workspace.status().diagnostic_exe_present);
    }

    #[test]
    fn import_rejects_unknown_without_creating_json() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        assert!(matches!(
            workspace.import_translations(),
            Err(ImportError::MissingInput)
        ));
        fs::write(dir.path().join(ORIGINALS).join("Script.pak"), b"not a pak").unwrap();
        assert!(matches!(
            workspace.import_translations(),
            Err(ImportError::Invalid(_))
        ));
        assert_eq!(workspace.status().translation_json_count, Some(0));
    }

    #[test]
    fn import_does_not_overwrite_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        let file = dir.path().join(TRANSLATIONS).join("my-work.json");
        fs::write(&file, b"user content").unwrap();
        fs::write(dir.path().join(ORIGINALS).join("Script.pak"), b"not a pak").unwrap();
        assert!(matches!(
            workspace.import_translations(),
            Err(ImportError::Occupied)
        ));
        assert_eq!(fs::read(file).unwrap(), b"user content");
    }

    #[test]
    fn import_known_pak_when_supplied_locally() {
        let Ok(path) = std::env::var("LAFINALE_TEST_PAK") else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        let original = dir.path().join(ORIGINALS).join("Script.pak");
        fs::copy(path, &original).unwrap();
        let summary = workspace.import_translations().unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                scripts: 117,
                strings: 77258
            }
        );
        assert_eq!(workspace.status().translation_json_count, Some(117));
        let json = fs::read_to_string(dir.path().join(TRANSLATIONS).join("scr00001.json")).unwrap();
        let document: kgo::Script = serde_json::from_str(&json).unwrap();
        assert_eq!(document.filename, "scr00001.kgo");
        assert_eq!(
            document.strings[0].japanese,
            "【大企業重役】　「……今度の契約が……」"
        );
        assert_eq!(document.strings[0].spanish, "");
        assert!(matches!(
            workspace.import_translations(),
            Err(ImportError::Occupied)
        ));
        assert_eq!(
            exe_patch::sha256(&fs::read(original).unwrap()),
            pak::ORIGINAL_SHA256
        );
    }

    #[test]
    fn pak_builder_does_not_overwrite_output() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        fs::write(dir.path().join(ORIGINALS).join("Script.pak"), b"bad").unwrap();
        let output = dir.path().join(OUTPUT).join("Script.pak");
        fs::write(&output, b"my output").unwrap();
        assert!(matches!(
            workspace.build_pak_diagnostic(),
            Err(PakBuildError::OutputExists)
        ));
        assert_eq!(fs::read(output).unwrap(), b"my output");
    }

    #[test]
    fn pak_builder_round_trip_when_sources_supplied_locally() {
        let (Ok(pak), Ok(json_dir)) = (
            std::env::var("LAFINALE_TEST_PAK"),
            std::env::var("LAFINALE_TEST_TRANSLATIONS"),
        ) else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        fs::copy(pak, dir.path().join(ORIGINALS).join("Script.pak")).unwrap();
        for file in fs::read_dir(json_dir).unwrap() {
            let file = file.unwrap();
            if file.path().extension() == Some(std::ffi::OsStr::new("json")) {
                fs::copy(
                    file.path(),
                    dir.path().join(TRANSLATIONS).join(file.file_name()),
                )
                .unwrap();
            }
        }
        let summary = workspace.build_pak_diagnostic().unwrap();
        assert_eq!(summary.report.translated_strings, 9673);
        assert_eq!(summary.report.wrapped_strings, 3577);
        assert_eq!(
            exe_patch::sha256(&fs::read(dir.path().join(OUTPUT).join("Script.pak")).unwrap()),
            summary.sha256
        );
        assert!(matches!(
            workspace.build_pak_diagnostic(),
            Err(PakBuildError::OutputExists)
        ));
    }

    #[test]
    fn pak_builder_rejects_unsafe_translation_without_output() {
        let Ok(pak) = std::env::var("LAFINALE_TEST_PAK") else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let workspace = Workspace::initialize(dir.path()).unwrap();
        fs::copy(pak, dir.path().join(ORIGINALS).join("Script.pak")).unwrap();
        workspace.import_translations().unwrap();
        let path = dir.path().join(TRANSLATIONS).join("scr00001.json");
        let mut document: kgo::Script = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        document.strings[0].spanish = "espacio　inseguro".into();
        fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        assert!(matches!(
            workspace.build_pak_diagnostic(),
            Err(PakBuildError::Invalid(_))
        ));
        assert!(!dir.path().join(OUTPUT).join("Script.pak").exists());
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
