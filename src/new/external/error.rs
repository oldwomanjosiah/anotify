use std::path::PathBuf;

#[derive(Debug)]
pub struct AnotifyError {
    pub(crate) message: Option<String>,
    pub(crate) backtrace: std::backtrace::Backtrace,
    pub(crate) path: Option<PathBuf>,
    pub(crate) ty: AnotifyErrorType,
}

#[derive(Debug)]
pub enum AnotifyErrorType {
    DoesNotExist,
    ExpectedDir,
    ExpectedFile,
    FileRemoved,
    SystemResourceLimit,
    NoPermission,
    InvalidFilePath,
    Unknown {
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

impl AnotifyError {
    pub(crate) fn new(ty: AnotifyErrorType) -> Self {
        Self {
            message: None,
            path: None,
            backtrace: std::backtrace::Backtrace::capture(),
            ty,
        }
    }

    pub(crate) fn attach_path(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        self.path.replace(path.into());
        self
    }

    pub(crate) fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path.replace(path.into());
        self
    }

    pub(crate) fn attach_message(&mut self, message: impl Into<String>) -> &mut Self {
        self.message.replace(message.into());
        self
    }

    pub(crate) fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message.replace(message.into());
        self
    }
}

pub type Result<T, E = AnotifyError> = core::result::Result<T, E>;

impl std::fmt::Display for AnotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            message,
            backtrace,
            path,
            ty,
        } = self;

        writeln!(f, "AnotifyError: {ty}")?;

        if let Some(ref message) = message {
            writeln!(f, "{message}")?;
        }

        if let Some(ref path) = path {
            writeln!(f, "For Path: {}", path.display())?;
        }

        if backtrace.status() == std::backtrace::BacktraceStatus::Captured {
            writeln!(f, "\nat:\n{backtrace}")?;
        }

        Ok(())
    }
}

impl AnotifyError {
    /// Get the backtrace for this error.
    ///
    /// Stability:
    /// This method is not considered stable, and will be removed as soon
    /// as the provider api for backtraces is stabalized.
    pub fn backtrace(&self) -> &std::backtrace::Backtrace {
        &self.backtrace
    }
}

impl std::error::Error for AnotifyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.ty {
            AnotifyErrorType::Unknown { source } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl std::fmt::Display for AnotifyErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnotifyErrorType::DoesNotExist => write!(f, "Does Not Exist"),
            AnotifyErrorType::ExpectedDir => write!(f, "Expected Directory"),
            AnotifyErrorType::ExpectedFile => write!(f, "Expected File"),
            AnotifyErrorType::FileRemoved => write!(f, "File was Removed"),
            AnotifyErrorType::SystemResourceLimit => {
                write!(f, "A System Resource Limit Would be Exceeded")
            }
            AnotifyErrorType::NoPermission => write!(f, "No Permission For Action"),
            AnotifyErrorType::InvalidFilePath => write!(f, "Invalid or Non-Existant Path"),
            AnotifyErrorType::Unknown { source } => write!(f, "Unknown({source})"),
        }
    }
}
