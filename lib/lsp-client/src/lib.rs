#[macro_use]
extern crate log;
use {
    lsp_types::{ClientCapabilities, InitializeParams, InitializeResult},
    std::error::Error,
    std::fmt,
    std::io,
    std::io::{BufRead, BufReader, Read, Write},
    std::ops::Add,
    std::path::PathBuf,
    std::process::{Child, ChildStdin, ChildStdout, Command, Stdio},
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
    workspace: String,
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
    JSONSerializationError(serde_json::Error),
    NotRunning,
    InvalidProcess,
    Other(&'static str),
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
            Self::StartError(..) => write!(f, "LSP start error"),
            Self::StopError(..) => write!(f, "LSP stop error"),
            Self::NotRunning => write!(f, "LSP is not running"),
            Self::InvalidProcess => write!(f, "Process is not initialized"),
            Self::JSONSerializationError(..) => write!(f, "Failed to serialize type to JSON"),
            Self::Other(s) => write!(f, "Error in LSPServer: {}", &s),
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
            Self::JSONSerializationError(e) => Some(e),
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
            process_id: None,
            root_path: Some(root_dir.to_owned()),
            root_uri: None,
            initialization_options: None,
            capabilities: ClientCapabilities::default(),
            trace: None,
            /// TODO: need workspace folders? support multiple projects with single LSP?
            workspace_folders: None,
        }
    }
}

impl LSPServer {
    fn new(language: Language, workspace: String) -> Self {
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
    fn initialize_lsp(&mut self) -> LSPResult {
        self.started()?;
        let request = LSPProtocol::initialize_request(&self.workspace);
        let mut req_ser =
            serde_json::to_string(&request).map_err(|e| LSPError::JSONSerializationError(e))?;
        req_ser = format!("Content-Length: {}\r\n{}", req_ser.len(), req_ser);
        let proc = self.process_mut()?;
        let stdin = proc
            .stdin
            .as_mut()
            .ok_or_else(|| LSPError::Other("stdin is unavailable"))?;

        let stdout: &mut ChildStdout = proc
            .stdout
            .as_mut()
            .ok_or_else(|| LSPError::Other("stdout is unavailable"))?;

        stdin
            .write_all(req_ser.as_bytes())
            .map_err(|_| LSPError::Other("Failed to write to LSP"))?;
        stdin.flush();
        let mut output = String::new();
        /// TODO: Find a way to read async. I want to manage messages from the LSP server in a
        /// non-blocking manner, this can be hacked manually through the RawFd but that would be
        /// a bad way
        let mut reader = BufReader::new(stdout);
        reader.read_line(&mut output).unwrap();
        /* let res: InitializeResult = serde_json::from_reader(stdout)
        .map_err(|_| LSPError::Other("Failed to decode JSON from LSP Server"))?; */
        error!("Initialize LSP Response: {:?}", output);
        Ok(())
    }

    fn started(&self) -> LSPResult {
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

#[cfg(test)]
mod test {
    use super::*;
    fn setup() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_sanity_lsp_server_start_stop() {
        setup();
        let mut lsp_server = LSPServer::new(Language::Rust, ".".into());
        lsp_server.start().unwrap();
        lsp_server.stop().unwrap();
    }

    #[test]
    fn test_sanity_lsp_server_start_restart() {
        setup();
        let mut lsp_server = LSPServer::new(Language::Rust, ".".into());
        lsp_server.start().unwrap();
        lsp_server.restart().unwrap();
    }

    #[test]
    fn test_lsp_initialize() {
        setup();
        let mut lsp_server = LSPServer::new(Language::Rust, ".".into());
        lsp_server.start().unwrap();
        lsp_server.initialize_lsp().unwrap();
    }
}
