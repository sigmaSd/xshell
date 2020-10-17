//! xshell makes it easy to write cross-platform "bash" scripts in Rust.
//!
//! It provides a `cmd!` macro for running subprocesses, as well as a number of
//! basic file manipulation utilities.
//!
//! ```
//! use xshell::{cmd, read_file};
//!
//! let name = "Julia";
//! let output = cmd!("echo hello {name}!").read()?;
//! assert_eq!(output, "hello Julia!");
//!
//! let err = read_file("feeling-lucky.txt").unwrap_err();
//! assert_eq!(
//!     err.to_string(),
//!     "`feeling-lucky.txt`: no such file or directory (os error 2)",
//! );
//! # Ok::<(), xshell::Error>(())
//! ```
//!
//! The intended use-case is various bits of glue code, which could be written
//! in bash or python. The original motivation is
//! [`xtask`](https://github.com/matklad/cargo-xtask) development.
//!
//! **Goals**: fast compile times, ergonomics, clear error messages.<br>
//! **Non goals**: completeness, robustness / misuse resistance.
//!
//! For "heavy-duty" code, consider using
//! [`duct`](https://github.com/oconnor663/duct.rs) or
//! [`std::process::Command`](https://doc.rust-lang.org/stable/std/process/struct.Command.html)
//! instead.
//!
//! # API Overview
//!
//! For a real-world example, see this crate's own CI script:
//!
//! [https://github.com/matklad/xshell/blob/master/examples/ci.rs](https://github.com/matklad/xshell/blob/master/examples/ci.rs)
//!
//! ## `cmd!` Macro
//!
//! Read output of the process into `String`. The final newline will be
//! stripped.
//!
//! ```
//! # use xshell::cmd;
//! let output = cmd!("date --iso").read()?;
//! assert!(output.chars().all(|c| "01234567890-".contains(c)));
//! # Ok::<(), xshell::Error>(())
//! ```
//!
//! If the exist status is non-zero, an error is returned.
//!
//! ```
//! # use xshell::cmd;
//! let err = cmd!("false").read().unwrap_err();
//! assert_eq!(
//!     err.to_string(),
//!     "command `false` failed, exit code: 1",
//! );
//! ```
//!
//! <hr>
//!
//! Run the process, inheriting stdout and stderr. The command is echoed to
//! stdout.
//!
//! ```
//! # use xshell::cmd;
//! cmd!("echo hello!").run()?;
//! # Ok::<(), xshell::Error>(())
//! ```
//!
//! Output
//!
//! ```text
//! $ echo hello!
//! hello!
//! ```
//!
//! <hr>
//!
//! Interpolation is supported via `{name}` syntax. Use `{name...}` to
//! interpolate sequence of values.
//!
//! ```
//! # use xshell::cmd;
//! let greeting = "Guten Tag";
//! let people = &["Spica", "Boarst", "Georgina"];
//! assert_eq!(
//!     cmd!("echo {greeting} {people...}").to_string(),
//!     r#"echo "Guten Tag" Spica Boarst Georgina"#
//! );
//! ```
//!
//! Splat syntax is used for optional argument idiom.
//!
//! ```
//! # use xshell::cmd;
//! let dry_run = if true { &["--dry-run"] } else { &[][..] };
//! assert_eq!(
//!     cmd!("git push {dry_run...}").to_string(),
//!     "git push --dry-run"
//! );
//! ```
//!
//! ## Manipulating the Environment
//!
//! Instead of `cd` and `export`, xshell uses RAII based `pushd` and `pushenv`
//!
//! ```
//! use xshell::{cwd, pushd, pushenv};
//!
//! let initial_dir = cwd()?;
//! {
//!     let _p = pushd("src")?;
//!     assert_eq!(
//!         cwd()?,
//!         initial_dir.join("src"),
//!     );
//! }
//! assert_eq!(cwd()?, initial_dir);
//!
//! assert!(std::env::var("MY_VAR").is_err());
//! let _e = pushenv("MY_VAR", "92");
//! assert_eq!(
//!     std::env::var("MY_VAR").as_deref(),
//!     Ok("92")
//! );
//! # Ok::<(), xshell::Error>(())
//! ```
//!
//! ## Working with Files
//!
//! xshell provides the following utilities, which are mostly re-exports from
//! `std::fs` module with paths added to error messages: `rm_rf`, `read_file`,
//! `write_file`, `mkdir_p`, `cp`, `read_dir`, `cwd`.
//!
//! # Maintenance
//!
//! Minimum Supported Rust Version: 1.47.0. MSRV bump is not considered semver
//! breaking. MSRV is updated conservatively.
//!
//! The crate isn't comprehensive. Additional functionality is added on
//! as-needed bases, as long as it doesn't compromise compile times.
//! Function-level docs are an especially welcome addition :-)
//!
//! # Implementation details
//!
//! The design is heavily inspired by the Juila language:
//!
//! * [Shelling Out Sucks](https://julialang.org/blog/2012/03/shelling-out-sucks/)
//! * [Put This In Your Pipe](https://julialang.org/blog/2013/04/put-this-in-your-pipe/)
//! * [Running External Programs](https://docs.julialang.org/en/v1/manual/running-external-programs/)
//! * [Filesystem](https://docs.julialang.org/en/v1/base/file/)
//!
//! Smaller influences are the [`duct`](https://github.com/oconnor663/duct.rs)
//! crate and Ruby's
//! [`FileUtils`](https://ruby-doc.org/stdlib-2.4.1/libdoc/fileutils/rdoc/FileUtils.html)
//! module.
//!
//! The `cmd!` macro uses a simple proc-macro internally. It doesn't depend on
//! helper libraries, so the fixed-cost impact on compile times is moderate.
//! Compiling a trivial program with `cmd!("date --iso")` takes one second.
//! Equivalent program using only `std::process::Command` compiles in 0.25
//! seconds.
//!
//! To make IDEs infer correct types without expanding proc-macro, it is wrapped
//! into a declarative macro which supplies type hints.
//!
//! Environment manipulation mutates global state and might have surprising
//! interactions with threads. Internally, everything is protected by a global
//! shell lock, so all functions in this crate are thread safe. However,
//! functions outside of xshell's control might experience race conditions:
//!
//! ```
//! use std::{thread, fs};
//!
//! use xshell::{pushd, read_file};
//!
//! let t1 = thread::spawn(|| {
//!     let _p = pushd("./src");
//! });
//!
//! // This is guaranteed to work: t2 will block while t1 is in `pushd`.
//! let t2 = thread::spawn(|| {
//!     let res = read_file("./src/lib.rs");
//!     assert!(res.is_ok());
//! });
//!
//! // This is a race: t3 might observe difference cwds depending on timing.
//! let t3 = thread::spawn(|| {
//!     let res = fs::read_to_string("./src/lib.rs");
//!     assert!(res.is_ok() || res.is_err());
//! });
//! # t1.join().unwrap(); t2.join().unwrap(); t3.join().unwrap();
//! ```
//!
//! # Naming
//!
//! xshell is an ex-shell, for those who grew tired of bash.<br>
//! xshell is an x-platform shell, for those who don't want to run `build.sh` on windows.<br>
//! xshell is built for [`xtask`](https://github.com/matklad/cargo-xtask).<br>
//! xshell uses x-traordinary level of [trickery](https://github.com/matklad/xshell/blob/843df7cd5b7d69fc9d2b884dc0852598335718fe/src/lib.rs#L233-L234),
//! just like `xtask` [does](https://matklad.github.io/2018/01/03/make-your-own-make.html).

mod env;
mod gsl;
mod error;
mod fs;

use std::{
    ffi::{OsStr, OsString},
    fmt, io,
    io::Write,
    path::Path,
    process::Output,
    process::Stdio,
};

use error::CmdErrorKind;
#[doc(hidden)]
pub use xshell_macros::__cmd;

pub use crate::{
    env::{pushd, pushenv, Pushd, Pushenv},
    error::{Error, Result},
    fs::{cp, cwd, mkdir_p, read_dir, read_file, rm_rf, write_file},
};

#[macro_export]
macro_rules! cmd {
    ($cmd:tt) => {{
        #[cfg(trick_rust_analyzer_into_highlighting_interpolated_bits)]
        format_args!($cmd);
        use $crate::Cmd as __CMD;
        let cmd: $crate::Cmd = $crate::__cmd!(__CMD $cmd);
        cmd
    }};
}

#[must_use]
#[derive(Debug)]
pub struct Cmd {
    args: Vec<OsString>,
    stdin_contents: Option<Vec<u8>>,
}

impl fmt::Display for Cmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut space = "";
        for arg in &self.args {
            write!(f, "{}", space)?;
            space = " ";

            let arg = arg.to_string_lossy();
            if arg.chars().any(|it| it.is_ascii_whitespace()) {
                write!(f, "\"{}\"", arg.escape_default())?
            } else {
                write!(f, "{}", arg)?
            };
        }
        Ok(())
    }
}

impl From<Cmd> for std::process::Command {
    fn from(cmd: Cmd) -> Self {
        cmd.command()
    }
}

impl Cmd {
    pub fn new(program: impl AsRef<Path>) -> Cmd {
        Cmd::_new(program.as_ref())
    }
    fn _new(program: &Path) -> Cmd {
        Cmd { args: vec![program.as_os_str().to_owned()], stdin_contents: None }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Cmd {
        self._arg(arg.as_ref());
        self
    }
    pub fn args<I>(mut self, args: I) -> Cmd
    where
        I: IntoIterator,
        I::Item: AsRef<OsStr>,
    {
        args.into_iter().for_each(|it| self._arg(it.as_ref()));
        self
    }
    fn _arg(&mut self, arg: &OsStr) {
        self.args.push(arg.to_owned())
    }

    #[doc(hidden)]
    pub fn __extend_arg(mut self, arg: impl AsRef<OsStr>) -> Cmd {
        self.___extend_arg(arg.as_ref());
        self
    }
    fn ___extend_arg(&mut self, arg: &OsStr) {
        self.args.last_mut().unwrap().push(arg)
    }

    pub fn stdin(mut self, stdin: impl AsRef<[u8]>) -> Cmd {
        self._stdin(stdin.as_ref());
        self
    }
    fn _stdin(&mut self, stdin: &[u8]) {
        self.stdin_contents = Some(stdin.to_vec());
    }

    pub fn read(self) -> Result<String> {
        {
            let s = Self::mrun(&self.args).unwrap();
            return Ok(s);
        }

        match self.read_raw() {
            Ok(output) if output.status.success() => {
                let mut stdout = String::from_utf8(output.stdout)
                    .map_err(|utf8_err| CmdErrorKind::NonUtf8Stdout(utf8_err).err(self))?;
                if stdout.ends_with('\n') {
                    stdout.pop();
                }

                Ok(stdout)
            }
            Ok(output) => Err(CmdErrorKind::NonZeroStatus(output.status).err(self)),
            Err(io_err) => Err(CmdErrorKind::Io(io_err).err(self)),
        }
    }
    fn read_raw(&self) -> io::Result<Output> {
        let mut child = self
            .command()
            .stdin(match &self.stdin_contents {
                Some(_) => Stdio::piped(),
                None => Stdio::null(),
            })
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        if let Some(stdin_contents) = &self.stdin_contents {
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(stdin_contents)?;
            stdin.flush()?;
        }
        child.wait_with_output()
    }

    pub fn run(self) -> Result<()> {
        println!("$ {}", self);
        match self.command().status() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(CmdErrorKind::NonZeroStatus(status).err(self)),
            Err(io_err) => Err(CmdErrorKind::Io(io_err).err(self)),
        }
    }

    fn command(&self) -> std::process::Command {
        let mut res = std::process::Command::new(&self.args[0]);
        res.args(&self.args[1..]);
        res
    }

    fn mrun(cmd: &[std::ffi::OsString]) -> std::io::Result<String> {
        use std::io::Read;
        use std::process;

        let cmd: Vec<&str> = cmd.iter().map(|c| c.to_str().unwrap()).collect();
        let cmd = &cmd;

        let mut stdin = None;

        let runit = |stdin: Option<process::Child>,
                     stdout: process::Stdio,
                     cmd: &[&str]|
         -> Option<process::Child> {
            if cmd.is_empty() {
                return None;
            }

            let mut cmd = cmd.iter();

            let stdin = if let Some(stdin) = stdin { stdin.stdout } else { None };

            if let Some(stdin) = stdin {
                if let Ok(child) = process::Command::new(cmd.next()?)
                    .args(&cmd.collect::<Vec<&&str>>())
                    .stdin(stdin)
                    .stdout(stdout)
                    .spawn()
                {
                    Some(child)
                } else {
                    None
                }
            } else if let Ok(child) = process::Command::new(cmd.next()?)
                .args(&cmd.collect::<Vec<&&str>>())
                .stdout(stdout)
                .spawn()
            {
                Some(child)
            } else {
                None
            }
        };

        let mut cmd = cmd.split(|c| c == &"|").peekable();
        while let Some(c) = cmd.next() {
            let stdout = if cmd.peek().is_some() {
                process::Stdio::piped()
            } else {
                process::Stdio::inherit()
            };
            stdin = runit(stdin, stdout, c);
        }
        // wait for the last command
        if let Some(process) = stdin.as_mut() {
            let _ = process.wait();
            let mut out = Vec::new();
            process.stdout.as_mut().unwrap().read_exact(&mut out).unwrap();

            return Ok(String::from_utf8_lossy(&out).to_string());
        } else {
            Ok(String::new())
        }
    }
}
