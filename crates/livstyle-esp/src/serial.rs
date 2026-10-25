//! Passive serial log capture. DTR and RTS are de-asserted right after open
//! and HUPCL is cleared, so neither open nor close drives the auto-reset
//! circuit. Some USB-UART bridges still glitch the lines at open; the board
//! profile notes which ones.

use std::io;
use std::path::Path;
use std::time::Duration;

#[cfg(unix)]
pub fn read_log(
    port: &Path,
    baud: u32,
    duration: Duration,
    max_bytes: usize,
) -> io::Result<Vec<u8>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::time::Instant;

    let speed = match baud {
        9600 => libc::B9600,
        57600 => libc::B57600,
        115200 => libc::B115200,
        230400 => libc::B230400,
        460800 => libc::B460800,
        921600 => libc::B921600,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsupported baud {baud}"),
            ));
        }
    };
    let path = CString::new(port.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "port path contains NUL"))?;
    // SAFETY: plain POSIX calls on a descriptor we own; closed on every path.
    unsafe {
        let fd = libc::open(
            path.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK | libc::O_CLOEXEC,
        );
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let close_err = |fd: i32| {
            let e = io::Error::last_os_error();
            libc::close(fd);
            e
        };
        let lines: libc::c_int = libc::TIOCM_DTR | libc::TIOCM_RTS;
        if libc::ioctl(fd, libc::TIOCMBIC, &lines) != 0 {
            return Err(close_err(fd));
        }
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut tio) != 0 {
            return Err(close_err(fd));
        }
        libc::cfmakeraw(&mut tio);
        tio.c_cflag &= !libc::HUPCL;
        tio.c_cflag |= libc::CLOCAL | libc::CREAD;
        libc::cfsetispeed(&mut tio, speed);
        libc::cfsetospeed(&mut tio, speed);
        if libc::tcsetattr(fd, libc::TCSANOW, &tio) != 0 {
            return Err(close_err(fd));
        }
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline && out.len() < max_bytes {
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(200) as i32;
            let r = libc::poll(&mut pfd, 1, remaining);
            if r < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                libc::close(fd);
                return Err(e);
            }
            if pfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                libc::close(fd);
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "serial port disconnected",
                ));
            }
            if r > 0 {
                let n = libc::read(fd, buf.as_mut_ptr().cast(), buf.len());
                if n > 0 {
                    out.extend_from_slice(&buf[..n as usize]);
                }
            }
        }
        libc::close(fd);
        out.truncate(max_bytes);
        Ok(out)
    }
}

#[cfg(not(unix))]
pub fn read_log(
    _port: &Path,
    _baud: u32,
    _duration: Duration,
    _max_bytes: usize,
) -> io::Result<Vec<u8>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "serial capture is not implemented on this platform yet",
    ))
}
