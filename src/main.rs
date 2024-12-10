#[allow(unused_imports)]
use std::io::{self, Write};
use std::{borrow::Cow, fs, str::FromStr};

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let stdin = io::stdin();
    write!(stdout, "$ ")?;
    stdout.flush()?;

    for line in stdin.lines() {
        let line = line?;
        let cmd = Cmd::from_str(&line).unwrap();
        cmd.execute(&mut stdout)?;
        write!(stdout, "$ ")?;
        stdout.flush()?;
    }
    Ok(())
}

trait ExecuteCmd {
    fn execute<W: io::Write>(&self, stdout: &mut W) -> io::Result<()>;
}

enum BuildinCmd<'a> {
    Exit(i32),
    Echo(Vec<Cow<'a, str>>),
    Type(Cow<'a, str>),
}

impl From<&BuildinCmd<'_>> for &str {
    fn from(value: &BuildinCmd) -> Self {
        match value {
            BuildinCmd::Exit(_) => "exit",
            BuildinCmd::Echo(_) => "echo",
            BuildinCmd::Type(_) => "type",
        }
    }
}

impl ExecuteCmd for BuildinCmd<'_> {
    fn execute<W: io::Write>(&self, stdout: &mut W) -> io::Result<()> {
        match self {
            Self::Exit(code) => std::process::exit(*code),
            Self::Echo(args) => {
                let mut iter = args.iter();
                if let Some(arg) = iter.next() {
                    write!(stdout, "{}", arg)?;
                }
                for arg in iter {
                    write!(stdout, " {}", arg)?;
                }
                writeln!(stdout)?;
            }
            Self::Type(arg) => {
                if let Ok(ref v) = BuildinCmd::from_str(arg) {
                    let v: &str = v.into();
                    writeln!(stdout, "{} is a shell builtin", v)?;
                    return Ok(());
                }
                let env = std::env::var("PATH").unwrap();
                for path in env.split(':') {
                    for entry in fs::read_dir(path)? {
                        let dir = entry?;
                        let file = dir.file_name();
                        let name = file.to_string_lossy();
                        if name == *arg {
                            writeln!(stdout, "{} is {}", arg, dir.path().to_string_lossy())?;
                            return Ok(());
                        }
                    }
                }
                writeln!(stdout, "{}: not found", arg)?;
            }
        }
        Ok(())
    }
}

impl FromStr for BuildinCmd<'_> {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut iter = s.split_whitespace();
        let err = Err("ERROR: Invalid input");
        let cmd = iter.next();
        if cmd.is_none() {
            return err;
        }
        match cmd.unwrap().trim() {
            "exit" => {
                let exit_status = iter.next();
                if exit_status.is_none() {
                    return Ok(Self::Exit(0));
                }
                if let Ok(code) = exit_status.unwrap().trim().parse() {
                    Ok(Self::Exit(code))
                } else {
                    err
                }
            }
            "echo" => Ok(Self::Echo(
                iter.map(|v| Cow::Owned(v.trim().to_owned())).collect(),
            )),
            "type" => {
                let arg = iter.next();
                Ok(Self::Type(Cow::Owned(arg.unwrap_or_default().to_owned())))
            }
            _ => err,
        }
    }
}

#[allow(unused)]
enum Cmd<'a> {
    Buildin(BuildinCmd<'a>),
    Other(Cow<'a, str>, Vec<Cow<'a, str>>),
}

impl FromStr for Cmd<'_> {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(buildin) = BuildinCmd::from_str(s) {
            return Ok(Self::Buildin(buildin));
        }
        let mut iter = s.split_whitespace();
        let err = Err("ERROR: Invalid input");
        let cmd = iter.next();
        if cmd.is_none() {
            return err;
        }
        Ok(Self::Other(
            Cow::Owned(cmd.unwrap().trim().to_owned()),
            iter.map(|v| Cow::Owned(v.trim().to_owned())).collect(),
        ))
    }
}

impl ExecuteCmd for Cmd<'_> {
    fn execute<W: io::Write>(&self, stdout: &mut W) -> io::Result<()> {
        match self {
            Self::Buildin(cmd) => cmd.execute(stdout),
            Self::Other(cmd, _) => writeln!(stdout, "{}: not found", cmd),
        }
    }
}
