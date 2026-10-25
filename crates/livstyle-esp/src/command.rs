use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandOutput {
    /// `None` when killed (timeout) or terminated by a signal.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }

    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Process execution seam so the runner can be tested without esptool.
pub trait Exec: Send {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> std::io::Result<CommandOutput>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemExec;

impl Exec for SystemExec {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> std::io::Result<CommandOutput> {
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn()?;
        let mut out_pipe = child.stdout.take().expect("piped");
        let mut err_pipe = child.stderr.take().expect("piped");
        let out_t = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = out_pipe.read_to_end(&mut s);
            s
        });
        let err_t = std::thread::spawn(move || {
            let mut s = Vec::new();
            let _ = err_pipe.read_to_end(&mut s);
            s
        });
        let start = Instant::now();
        let (status, timed_out) = loop {
            if let Some(st) = child.try_wait()? {
                break (st.code(), false);
            }
            if start.elapsed() >= timeout {
                // Kill the whole group: esptool wrappers may fork, and orphans
                // would keep the serial port and our pipes open.
                #[cfg(unix)]
                // SAFETY: signalling a process group we created.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                break (None, true);
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        Ok(CommandOutput {
            status,
            stdout: String::from_utf8_lossy(&out_t.join().unwrap_or_default()).into_owned(),
            stderr: String::from_utf8_lossy(&err_t.join().unwrap_or_default()).into_owned(),
            timed_out,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_and_times_out() {
        let sh = Path::new("/bin/sh");
        let ok = SystemExec
            .run(
                sh,
                &["-c".into(), "echo hi; echo err >&2; exit 3".into()],
                Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            (ok.status, ok.stdout.as_str(), ok.stderr.as_str()),
            (Some(3), "hi\n", "err\n")
        );
        let slow = SystemExec
            .run(
                sh,
                &["-c".into(), "sleep 5".into()],
                Duration::from_millis(200),
            )
            .unwrap();
        assert!(slow.timed_out && slow.status.is_none());
    }
}
