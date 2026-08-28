use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

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
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        receiver
    });
    let stderr = child.stderr.take().map(|mut stream| {
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        receiver
    });

    let started_at = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = stdout
                .and_then(|receiver| receiver.recv_timeout(OUTPUT_DRAIN_TIMEOUT).ok())
                .unwrap_or_default();
            let stderr = stderr
                .and_then(|receiver| receiver.recv_timeout(OUTPUT_DRAIN_TIMEOUT).ok())
                .unwrap_or_default();
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }

        if started_at.elapsed() >= timeout_after {
            let pid = child.id();
            // `cmd /C`, PowerShell, and a few vendor probes can create a
            // grandchild that inherits the stdout/stderr pipe. Killing only
            // the direct child leaves the drainer threads blocked until that
            // grandchild exits naturally, which defeats the timeout's
            // purpose and can keep the test/application alive for seconds.
            // Ask Windows to terminate the scoped process tree asynchronously
            // while retaining the direct-child fallback below. The helper is
            // deliberately detached so the caller's latency budget is not
            // extended by a best-effort cleanup command.
            // Process creation itself can stall under Windows runner load, so
            // both spawning `taskkill` and reaping the direct child must live
            // outside the caller's latency budget. Keeping the tree kill and
            // direct-child fallback in one cleanup thread also preserves their
            // ordering: taskkill sees the parent PID before Child::kill can
            // remove it from the process table.
            thread::spawn(move || {
                #[cfg(windows)]
                {
                    let mut tree_kill = Command::new("taskkill");
                    tree_kill
                        .args(["/F", "/T", "/PID", &pid.to_string()])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null());
                    use std::os::windows::process::CommandExt;
                    tree_kill.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                    if let Ok(mut tree_kill) = tree_kill.spawn() {
                        let _ = tree_kill.wait();
                    }
                }

                let _ = child.kill();
                let _ = child.wait();
            });
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

    #[test]
    fn command_timeout_does_not_wait_for_the_childs_full_duration() {
        let command = if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", "ping -n 6 127.0.0.1 > NUL"]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", "sleep 5"]);
            cmd
        };

        let started_at = Instant::now();
        let error =
            command_output_with_timeout(command, Duration::from_millis(50), "timeout regression")
                .expect_err("the long-running command must time out");

        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(
            started_at.elapsed() < Duration::from_secs(1),
            "timeout helper waited too long: {:?}",
            started_at.elapsed()
        );
    }
}
