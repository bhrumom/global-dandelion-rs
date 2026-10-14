//! Whitelisted, argv-only, timeout/output-bounded NetworkManager adapter.
use dandelion_core::{
    Error, Result,
    process::{Output, ProcessRunner, Program, Request},
};
use std::process::Stdio;
use tokio::{io::AsyncReadExt, process::Command};
const CAP: u64 = 64 * 1024;
pub struct NativeProcess;
impl ProcessRunner for NativeProcess {
    async fn run(&self, request: Request) -> Result<Output> {
        if request.arguments.len() > 128
            || request.arguments.iter().any(|s| s.len() > 4096)
            || request.timeout.is_zero()
            || request.timeout.as_secs() > 30
        {
            return Err(Error::Config("invalid process request bounds".into()));
        }
        let executable = match request.program {
            Program::NetworkManager => "/usr/bin/nmcli",
        };
        let mut child = Command::new(executable)
            .args(&request.arguments)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Platform("stdout capture unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Platform("stderr capture unavailable".into()))?;
        let capture = async {
            let out = async {
                let mut bytes = Vec::new();
                stdout.take(CAP + 1).read_to_end(&mut bytes).await?;
                if bytes.len() as u64 > CAP {
                    return Err(std::io::Error::other("stdout exceeded limit"));
                }
                Ok::<_, std::io::Error>(bytes)
            };
            let err = async {
                let mut bytes = Vec::new();
                stderr.take(CAP + 1).read_to_end(&mut bytes).await?;
                if bytes.len() as u64 > CAP {
                    return Err(std::io::Error::other("stderr exceeded limit"));
                }
                Ok::<_, std::io::Error>(())
            };
            let (status, stdout, ()) = tokio::try_join!(child.wait(), out, err)?;
            Ok::<_, std::io::Error>(Output {
                code: status.code().unwrap_or(-1),
                stdout,
            })
        };
        match tokio::time::timeout(request.timeout, capture).await {
            Ok(Ok(output)) => Ok(output),
            other => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                match other {
                    Ok(Err(e)) => Err(e.into()),
                    Err(_) => Err(Error::Platform(
                        "NetworkManager command timed out and was terminated".into(),
                    )),
                    _ => Err(Error::Platform("unexpected process outcome".into())),
                }
            }
        }
    }
}
