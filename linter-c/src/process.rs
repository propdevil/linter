use linter::Error;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

pub(crate) struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(crate) fn run(
    command: &mut Command,
    root: &Path,
    timeout_ms: u64,
    max_output_bytes: u64,
) -> Result<Output, Error> {
    let mut stdout = tempfile::tempfile().map_err(failure)?;
    let mut stderr = tempfile::tempfile().map_err(failure)?;
    let mut child = command
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().map_err(failure)?)
        .stderr(stderr.try_clone().map_err(failure)?)
        .spawn()
        .map_err(failure)?;
    let started = Instant::now();
    loop {
        let result = poll(
            &mut child,
            &stdout,
            &stderr,
            started,
            timeout_ms,
            max_output_bytes,
        );
        match result {
            Ok(Some(status)) => {
                return Ok(Output {
                    status,
                    stdout: read(&mut stdout, max_output_bytes)?,
                    stderr: read(&mut stderr, max_output_bytes)?,
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
    }
}

fn poll(
    child: &mut std::process::Child,
    stdout: &File,
    stderr: &File,
    started: Instant,
    timeout_ms: u64,
    max_output_bytes: u64,
) -> Result<Option<ExitStatus>, Error> {
    let bytes = stdout
        .metadata()
        .map_err(failure)?
        .len()
        .saturating_add(stderr.metadata().map_err(failure)?.len());
    if bytes > max_output_bytes {
        return Err(Error::Analysis(
            "external tool exceeded its output limit".into(),
        ));
    }
    if started.elapsed() >= Duration::from_millis(timeout_ms) {
        return Err(Error::Analysis("external tool exceeded its timeout".into()));
    }
    child.try_wait().map_err(failure)
}

fn read(file: &mut File, limit: u64) -> Result<Vec<u8>, Error> {
    file.seek(SeekFrom::Start(0)).map_err(failure)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Analysis(
            "external tool exceeded its output limit".into(),
        ));
    }
    Ok(bytes)
}

fn failure(error: std::io::Error) -> Error {
    Error::Analysis(format!("external tool: {error}"))
}
