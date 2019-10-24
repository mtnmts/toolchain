use {
    lsp_types::{ClientCapabilities, InitializeParams},
    std::error::Error,
    std::fmt,
    std::io,
    std::path::PathBuf,
    std::process::{Child, Command, Stdio},
};

type LSPResult = Result<(), LSPError>;

pub enum Language {
    Cpp,
    C,
    Python,
    Rust,
}

impl Language {
    fn program(&self) -> &str {
        match self {
            Self::C | Self::Cpp => "clangd",
            Self::Rust => "rls",
            Self::Python => "pyls",
        }
    }
}

struct LSPServer {
    language: Language,
    workspace: PathBuf,
    started: bool,
    process: Option<Child>,
}

#[derive(Debug)]
enum LSPStartError {
    SpawnFailed(io::Error),
    AlreadyStarted,
}

unsafe impl Send for LSPStartError {}
unsafe impl Sync for LSPStartError {}

#[derive(Debug)]
enum LSPStopError {
    FailedToKill(io::Error),
}

unsafe impl Send for LSPStopError {}
unsafe impl Sync for LSPStopError {}

#[derive(Debug)]
enum LSPError {
    StartError(LSPStartError),
    StopError(LSPStopError),
    NotRunning,
    InvalidProcess,
}

unsafe impl Send for LSPError {}
unsafe impl Sync for LSPError {}

impl fmt::Display for LSPStopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FailedToKill(e) => write!(f, "Failed to kill process, reason: {}", e),
        }
    }
}

impl fmt::Display for LSPStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(e) => write!(f, "Failed to spawn LSP process, reason: {}", e),
            Self::AlreadyStarted => write!(f, "LSP already started"),
        }
    }
}

impl fmt::Display for LSPError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StartError(_) => write!(f, "LSP start error"),
            Self::StopError(_) => write!(f, "LSP stop error"),
            Self::NotRunning => write!(f, "LSP is not running"),
            Self::InvalidProcess => write!(f, "Process is not initialized"),
        }
    }
}

impl Error for LSPStopError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FailedToKill(e) => Some(e),
            _ => None,
        }
    }
}

impl Error for LSPStartError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SpawnFailed(e) => Some(e),
            _ => None,
        }
    }
}

impl Error for LSPError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StartError(e) => Some(e),
            Self::StopError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<LSPStartError> for LSPError {
    fn from(error: LSPStartError) -> LSPError {
        LSPError::StartError(error)
    }
}

impl From<LSPStopError> for LSPError {
    fn from(error: LSPStopError) -> LSPError {
        LSPError::StopError(error)
    }
}

struct LSPProtocol {}
type InitializeRequest = InitializeParams;
impl LSPProtocol {
    fn initialize_request(root_dir: &str) -> InitializeRequest {
        InitializeParams { 
            process_id : None, 
            root_path : Some(root_dir.to_owned()), 
            root_uri: None,
            initialization_options : None, 
            capabilities : ClientCapabilities::default(),
            trace: None,
            /// TODO: need workspace folders? support multiple projects with single LSP?
            workspace_folders: None,
        }
    } 
}

impl LSPServer {
    fn new(language: Language, workspace: PathBuf) -> Self {
        Self {
            language,
            workspace,
            started: false,
            process: None,
        }
    }

    fn process(&self) -> Result<&Child, LSPError> {
        self.process.as_ref().ok_or(LSPError::InvalidProcess)
    }
   
    fn process_mut(&mut self) -> Result<&mut Child, LSPError> {
        self.process.as_mut().ok_or(LSPError::InvalidProcess)
    }
    async fn _initialize_lsp(&self) -> LSPResult {
        self.started()?;
        let _proc = self.process()?;
        let stdin = _proc.stdin.as_ref().unwrap();
        Ok(())
    }
    fn started(&self) -> Result<(), LSPError> {
        match self.started {
            true => Ok(()),
            false => Err(LSPError::NotRunning),
        }
    }
    fn start(&mut self) -> LSPResult {
        if self.started {
            return Err(LSPStartError::AlreadyStarted.into());
        }
        let prog = self.language.program();
        match Command::new(prog)
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(proc) => {
                self.started = true;
                self.process = Some(proc);
                Ok(())
            }
            Err(e) => Err(LSPStartError::SpawnFailed(e).into()),
        }
    }

    fn stop(&mut self) -> LSPResult {
        self.started()?;
        self.process_mut()?
            .kill()
            .map_err(|e| LSPError::StopError(LSPStopError::FailedToKill(e)))?;
        self.process = None;
        self.started = false;
        Ok(())
    }

    fn restart(&mut self) -> LSPResult {
        self.stop()?;
        self.start()?;
        Ok(())
    }
}

mod test {
    use super::*;
    #[test]
    fn test_sanity_lsp_server_start_stop() {
        let mut lsp_server = LSPServer::new(Language::Rust, ".".into());
        lsp_server.start().unwrap();
        lsp_server.stop().unwrap();
    }

    #[test]
    fn test_sanity_lsp_server_start_restart() {
        let mut lsp_server = LSPServer::new(Language::Rust, ".".into());
        lsp_server.start().unwrap();
        lsp_server.restart().unwrap();
    }
}
