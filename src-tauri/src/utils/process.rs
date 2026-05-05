use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Run a child process and collect output with a hard timeout.
///
/// This avoids unbounded waits on FFmpeg/ffprobe/where/which probes while also
/// draining stdout/stderr so normal command output cannot deadlock the process.
pub fn command_output_with_timeout(
    mut command: Command,
    timeout_after: Duration,
    operation: &str,
) -> io::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;

    let stdout = child.stdout.take().map(|mut stream| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.read_to_end(&mut bytes);
            bytes
        })
    });
    let stderr = child.stderr.take().map(|mut stream| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.read_to_end(&mut bytes);
            bytes
        })
    });

    let started_at = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = stdout
                .and_then(|handle| handle.join().ok())
                .unwrap_or_default();
            let stderr = stderr
                .and_then(|handle| handle.join().ok())
                .unwrap_or_default();
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }

        if started_at.elapsed() >= timeout_after {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.map(|handle| handle.join());
            let _ = stderr.map(|handle| handle.join());
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "{} timed out after {} seconds",
                    operation,
                    timeout_after.as_secs()
                ),
            ));
        }

        thread::sleep(Duration::from_millis(50));
    }
}

pub fn command_status_success_with_timeout(
    mut command: Command,
    timeout_after: Duration,
    operation: &str,
) -> io::Result<bool> {
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let output = command_output_with_timeout(command, timeout_after, operation)?;
    Ok(output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_output_collects_stdout() {
        let command = if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "echo hello"]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", "printf hello"]);
            cmd
        };

        let output =
            command_output_with_timeout(command, Duration::from_secs(5), "test command").unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("hello"));
    }
}
