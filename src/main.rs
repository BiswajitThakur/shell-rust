use std::io::{self, BufWriter, Write};
use std::iter::{Enumerate, Peekable};
use std::process::Stdio;
use std::str::Chars;
use std::{borrow::Cow, fmt, fs, path::PathBuf, process, str::FromStr};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    print!("$ ");
    io::stdout().flush()?;

    for line in stdin.lines() {
        let line = line?;
        if line.trim().is_empty() {
            print!("$ ");
            io::stdout().flush()?;
            continue;
        }
        let (redirect_path, args) = get_redirect_path(IterArgs::new(line.as_str()).collect())?;
        let cmd = Cmd::from(args);
        cmd.execute(redirect_path)?;
        print!("$ ");
        io::stdout().flush()?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Cmd<'a> {
    Exit(i32),
    Echo(Vec<Cow<'a, str>>),
    Type(Cow<'a, str>),
    Pwd,
    Cd(Cow<'a, str>),
    Other(Cow<'a, str>, Vec<Cow<'a, str>>),
}

impl fmt::Display for Cmd<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exit(_) => f.write_str("exit")?,
            Self::Echo(_) => f.write_str("echo")?,
            Self::Type(_) => f.write_str("type")?,
            Self::Pwd => f.write_str("pwd")?,
            Self::Cd(_) => f.write_str("cd")?,
            Self::Other(cmd, _) => {
                if let Some(path) = find_path(cmd) {
                    return write!(f, "{} is {}", cmd, path);
                } else {
                    return write!(f, "{}: not found", cmd);
                }
            }
        };
        f.write_str(" is a shell builtin")
    }
}

impl Cmd<'_> {
    fn is_builtin(&self) -> bool {
        !matches!(self, Self::Other(_, _))
    }
}

impl<'a> Cmd<'a> {
    #[allow(unused)]
    fn execute(&'a self, out: Redirection<'_>) -> io::Result<()> {
        let mut stdout = BufWriter::new(out.stdout()?);
        let mut stderr = BufWriter::new(out.stderr()?);
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
                let arg = match arg {
                    Cow::Owned(v) => v,
                    Cow::Borrowed(v) => *v,
                };
                let cmd = Self::from(arg);
                if cmd.is_builtin() {
                    writeln!(stdout, "{}", cmd)?;
                    return Ok(());
                }
                if let Some(v) = find_path(arg) {
                    writeln!(stdout, "{} is {}", arg, v)?;
                    return Ok(());
                }
                writeln!(stdout, "{}: not found", arg)?;
            }
            Self::Pwd => {
                let pwd = std::env::current_dir()?;
                writeln!(stdout, "{}", pwd.to_string_lossy())?;
            }
            Self::Cd(path) => {
                if *path == "~" {
                    let home = std::env::var("HOME").unwrap();
                    std::env::set_current_dir(home)?;
                } else if std::env::set_current_dir(PathBuf::from_str(path).unwrap()).is_err() {
                    writeln!(stdout, "cd: {}: No such file or directory", path)?;
                }
            }
            Self::Other(cmd, args) => {
                if find_path(cmd).is_some() {
                    let mut child = process::Command::new(cmd.as_ref())
                        .args(args.iter().map(|v| v.as_ref()).collect::<Vec<&str>>())
                        .stdout(Stdio::from(out.stdout()?))
                        .stderr(Stdio::from(out.stderr()?))
                        .spawn()?;
                    let _ = child.wait()?;
                } else {
                    writeln!(stdout, "{}: command not found", cmd)?;
                }
            }
        }
        Ok(())
    }
}

impl<'a> From<&'a str> for Cmd<'a> {
    fn from(value: &'a str) -> Self {
        let value = value.trim_start();
        let mut cmd_args = IterArgs::new(value);
        let cmd = cmd_args.next().unwrap();
        match cmd.as_ref() {
            "exit" => {
                let code = cmd_args.next().unwrap_or_default();
                Self::Exit(code.parse().unwrap_or_default())
            }
            "echo" => Self::Echo(cmd_args.collect()),
            "type" => Self::Type(cmd_args.next().unwrap_or_default()),
            "pwd" => Self::Pwd,
            "cd" => Self::Cd(cmd_args.next().unwrap_or(Cow::Borrowed("~"))),
            _ => Self::Other(cmd, cmd_args.collect()),
        }
    }
}
impl<'a> From<Vec<Cow<'a, str>>> for Cmd<'a> {
    fn from(value: Vec<Cow<'a, str>>) -> Self {
        let mut iter = value.into_iter();
        let cmd = iter.next().unwrap();
        match cmd.as_ref() {
            "exit" => {
                let code = iter.next().unwrap_or_default();
                Self::Exit(code.parse().unwrap_or_default())
            }
            "echo" => Self::Echo(iter.collect()),
            "type" => Self::Type(iter.next().unwrap_or_default()),
            "pwd" => Self::Pwd,
            "cd" => Self::Cd(iter.next().unwrap_or(Cow::Borrowed("~"))),
            _ => Self::Other(cmd, iter.collect()),
        }
    }
}
fn find_path<T: AsRef<str>>(value: T) -> Option<String> {
    let env = std::env::var("PATH").unwrap();
    for path in env.split(':') {
        for entry in fs::read_dir(path).ok()? {
            let dir = entry.ok()?;
            let file = dir.file_name();
            let name = file.to_string_lossy();
            if name == *value.as_ref() {
                return Some(dir.path().to_string_lossy().to_string());
            }
        }
    }
    None
}

struct IterArgs<'a> {
    whole: &'a str,
    start: usize,
}

impl<'a> Iterator for IterArgs<'a> {
    type Item = Cow<'a, str>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.start >= self.whole.len() {
                return None;
            }
            let input = &self.whole[self.start..];
            let mut end = 0;
            let mut rm = Vec::new();
            handle_args(&mut input.chars().enumerate().peekable(), &mut rm, &mut end);
            let got_str = remove_unwanted(&input[0..end], rm);
            self.start += end;
            if got_str.is_empty() && end >= self.whole.len() {
                return None;
            }
            if got_str.is_empty() {
                continue;
            }
            return Some(got_str);
        }
    }
}
impl<'a> IterArgs<'a> {
    fn new(value: &'a str) -> Self {
        Self {
            whole: value,
            start: 0,
        }
    }
}

// BUG: in some input it return Owned value, when it should be Borrowed
fn remove_unwanted(value: &str, remove: Vec<usize>) -> Cow<'_, str> {
    if remove.is_empty() || value.is_empty() {
        return Cow::Borrowed(value);
    }
    let mut start = 0;
    for i in remove.iter() {
        if *i != start {
            break;
        }
        start += 1;
    }
    let mut end = value.len() - 1;
    for item in remove.iter().rev() {
        if *item != end {
            break;
        }
        end -= 1;
    }
    if start + (value.len() - 1 - end) >= value.len() {
        return Cow::Borrowed("");
    }
    if start + (value.len() - 1 - end) >= remove.len() {
        return Cow::Borrowed(&value[start..end + 1]);
    }
    let mut st = String::with_capacity(end - start + 1);
    let mut remove_iter = remove[start..].iter();
    let mut current_remove = remove_iter.next();
    for (index, c) in value[start..end + 1].chars().enumerate() {
        match current_remove {
            Some(remove_index) if *remove_index == index + start => {
                current_remove = remove_iter.next();
            }
            _ => st.push(c),
        }
    }
    Cow::Owned(st)
}
fn handle_args(iter: &mut Peekable<Enumerate<Chars>>, remove: &mut Vec<usize>, end: &mut usize) {
    if iter.peek().is_none() {
        return;
    }
    let mut i = 0;
    while let Some((index, c)) = iter.next() {
        i = index;
        match c {
            ' ' | '\t' | '\r' => {
                remove.push(index);
                *end = index + 1;
                return;
            }
            '\\' => {
                remove.push(index);
                iter.next();
                i += 1;
            }
            '"' => {
                remove.push(index);
                while let Some((ii, v)) = iter.next() {
                    i = ii;
                    match v {
                        '"' => {
                            remove.push(ii);
                            break;
                        }
                        '\\' => {
                            if let Some((_, v)) = iter.peek() {
                                if matches!(*v, '\\' | '"') {
                                    remove.push(ii);
                                    iter.next();
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            '\'' => {
                remove.push(index);
                for (ii, v) in iter.by_ref() {
                    i = ii;
                    if v == '\'' {
                        remove.push(ii);
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    *end = i + 1;
}

#[derive(Debug)]
enum RedirOps {
    Redirect,
    Append,
}

#[derive(Debug)]
struct RedirectPath<'a> {
    path: Cow<'a, str>,
    ops: RedirOps,
}

impl RedirectPath<'_> {
    fn default_stdout() -> Self {
        Self {
            path: Cow::Borrowed("/dev/stdout"),
            ops: RedirOps::Append,
        }
    }
    fn default_stderr() -> Self {
        RedirectPath {
            path: Cow::Borrowed("/dev/stderr"),
            ops: RedirOps::Append,
        }
    }
}

#[derive(Debug)]
struct Redirection<'a> {
    std_out: RedirectPath<'a>,
    std_err: RedirectPath<'a>,
}

impl Default for Redirection<'_> {
    fn default() -> Self {
        Self {
            std_out: RedirectPath::default_stdout(),
            std_err: RedirectPath::default_stderr(),
        }
    }
}

impl Redirection<'_> {
    fn stdout(&self) -> io::Result<fs::File> {
        match self.std_out.ops {
            RedirOps::Append => Ok(fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(self.std_out.path.as_ref())?),
            RedirOps::Redirect => Ok(fs::File::create(self.std_out.path.as_ref())?),
        }
    }
    fn stderr(&self) -> io::Result<fs::File> {
        match self.std_err.ops {
            RedirOps::Append => Ok(fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(self.std_err.path.as_ref())?),
            RedirOps::Redirect => Ok(fs::File::create(self.std_err.path.as_ref())?),
        }
    }
}

fn get_redirect_path(args: Vec<Cow<'_, str>>) -> io::Result<(Redirection<'_>, Vec<Cow<'_, str>>)> {
    let mut args1 = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    let mut stdout_path = None;
    let mut stdout_ops = RedirOps::Append;
    let mut stderr_path = None;
    let mut stderr_ops = RedirOps::Append;
    while let Some(arg) = iter.next() {
        match arg.as_ref() {
            ">" | "1>" => {
                if stdout_path.is_none() {
                    stdout_path = iter.next();
                    stdout_ops = RedirOps::Redirect;
                }
            }
            ">>" | "1>>" => {
                if stderr_path.is_none() {
                    stdout_path = iter.next();
                }
            }
            "2>" => {
                if stderr_path.is_none() {
                    stderr_path = iter.next();
                    stderr_ops = RedirOps::Redirect;
                }
            }
            "2>>" => {
                if stderr_path.is_none() {
                    stderr_path = iter.next();
                }
            }
            _ => args1.push(arg),
        }
    }
    Ok((
        Redirection {
            std_out: RedirectPath {
                path: stdout_path.unwrap_or(Cow::Borrowed("/dev/stdout")),
                ops: stdout_ops,
            },
            std_err: RedirectPath {
                path: stderr_path.unwrap_or(Cow::Borrowed("/dev/stderr")),
                ops: stderr_ops,
            },
        },
        args1,
    ))
}
