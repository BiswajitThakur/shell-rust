use std::io::{self, BufReader, Read, Write};
use std::{borrow::Cow, fmt, fs, path::PathBuf, process, str::FromStr};

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let stdin = io::stdin();
    write!(stdout, "$ ")?;
    stdout.flush()?;

    for line in stdin.lines() {
        let line = line?;
        if !line.trim().is_empty() {
            let cmd = Cmd::from(line.as_str());
            cmd.execute(&mut stdout)?;
        }
        write!(stdout, "$ ")?;
        stdout.flush()?;
    }
    Ok(())
}

trait ExecuteCmd<'a> {
    fn execute<W: io::Write>(&'a self, stdout: &mut W) -> io::Result<()>;
}

#[allow(unused)]
#[derive(Debug, PartialEq, Eq)]
enum Cmd<'a> {
    Exit(i32),
    Echo(Vec<Cow<'a, str>>),
    Type(Cow<'a, str>),
    Pwd,
    Cd(Cow<'a, str>),
    Cat(Vec<Cow<'a, str>>),
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
            Self::Cat(_) => f.write_str("cat")?,
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
        !matches!(self, Self::Other(_, _) | Self::Cat(_))
    }
}

impl<'a> ExecuteCmd<'a> for Cmd<'a> {
    fn execute<W: io::Write>(&'a self, stdout: &mut W) -> io::Result<()> {
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
            Self::Cat(args) => {
                for path in args {
                    let path = match path {
                        Cow::Borrowed(v) => *v,
                        Cow::Owned(v) => v,
                    };
                    let file = fs::File::open(path)?;
                    let mut reader = BufReader::new(file);
                    let mut buffer: [u8; 1024] = [0; 1024];
                    loop {
                        let r = reader.read(&mut buffer)?;
                        if r == 0 {
                            break;
                        }
                        stdout.write_all(&buffer[..r])?;
                    }
                }
            }
            Self::Other(cmd, args) => {
                if let Some(path) = find_path(cmd) {
                    process::Command::new(path).arg(args.join(" ")).status()?;
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
        value
            .strip_prefix("exit")
            .and_then(|rest| {
                if rest.trim_start().is_empty() {
                    Some(Self::Exit(0))
                } else if rest.chars().next().unwrap().is_whitespace() {
                    let mut iter = rest.split_whitespace();
                    let next_word = iter.next().unwrap_or_default();
                    if iter.next().is_some() {
                        panic!("exit: too many arguments")
                    } else {
                        Some(Self::Exit(next_word.parse().unwrap_or_default()))
                    }
                } else {
                    None
                }
            })
            .or(value.strip_prefix("echo").and_then(|rest| {
                if !rest.is_empty() && !rest.chars().next().unwrap().is_whitespace() {
                    return None;
                }
                Some(Self::Echo(parse_args(rest)))
            }))
            .or(value
                .strip_prefix("type ")
                .map(|rest| Self::Type(Cow::Borrowed(rest))))
            .or({
                if value.trim() == "type" {
                    Some(Self::Type("type".into()))
                } else {
                    None
                }
            })
            .or(value.strip_prefix("pwd").and_then(|rest| {
                if rest.is_empty() || rest.chars().next().unwrap().is_whitespace() {
                    Some(Self::Pwd)
                } else {
                    None
                }
            }))
            .or(value.strip_prefix("cd").and_then(|rest| {
                if !rest.is_empty() && !rest.chars().next().unwrap().is_whitespace() {
                    return None;
                }
                let args = parse_args(rest);
                if args.len() > 1 {
                    panic!("cd: too many arguments");
                }
                Some(Self::Cd(if args.is_empty() {
                    Cow::Borrowed("~")
                } else {
                    args.into_iter().next().unwrap()
                }))
            }))
            .or(value.strip_prefix("cat").and_then(|rest| {
                if !rest.is_empty() && !rest.chars().next().unwrap().is_whitespace() {
                    return None;
                }
                Some(Self::Cat(parse_args(rest)))
            }))
            .or({
                let mut args = parse_args(value).into_iter();
                let cmd = args.next().unwrap();
                Some(Self::Other(cmd, args.collect()))
            })
            .unwrap()
    }
}

fn parse_args<'a>(value: &'a str) -> Vec<Cow<'a, str>> {
    let mut v: Vec<Cow<'a, str>> = Vec::new();
    let value = value.trim_start();
    let mut iter = value.chars().enumerate().peekable();
    while let Some((index, c)) = iter.next() {
        match c {
            ' ' | '\r' | '\t' => {}
            '"' => {
                let start = index + 1;
                while let Some((i, c)) = iter.peek() {
                    if *c == '\\' {
                        iter.next();
                        continue;
                    }
                    if *c == '"' {
                        if start >= *i {
                            v.push(Cow::Borrowed(""));
                        } else {
                            v.push(Cow::Borrowed(&value[start..*i]));
                        }
                        iter.next();
                        break;
                    }
                    iter.next();
                }
            }
            '\'' => {
                let start = index + 1;
                while let Some((i, c)) = iter.peek() {
                    if *c == '\\' {
                        iter.next();
                        continue;
                    }
                    if *c == '\'' {
                        if start < *i {
                            v.push(Cow::Borrowed(&value[start..*i]));
                        }
                        iter.next();
                        break;
                    }
                    iter.next();
                }
            }
            _ => {
                while let Some((i, c)) = iter.peek() {
                    let i = *i;
                    if *c == '\\' {
                        iter.next();
                        iter.next();
                        continue;
                    }
                    if c.is_whitespace() || matches!(*c, '"' | '\'') {
                        let end = i;
                        if index < end {
                            v.push(Cow::Borrowed(&value[index..end]));
                        }
                        break;
                    }
                    iter.next();
                    if iter.peek().is_none() {
                        let end = i + 1;
                        if index < end {
                            let s = &value[index..end];
                            if s.contains("\\") {
                                v.push(Cow::Owned(s.replace("\\", "")));
                            } else {
                                v.push(Cow::Borrowed(s));
                            }
                        }
                    }
                }
            }
        }
    }
    v
}
#[test]
fn test_parse_args() {
    let input = "  hello'hiin  gjg\"chh  'hfh\\y\" gnm \"fhf '  '  gg\"\" hhf   ee";
    let want = vec![
        "hello",
        "hiin  gjg\"chh  ",
        "hfh\\y",
        " gnm ",
        "fhf",
        "  ",
        "gg",
        "",
        "hhf",
        "ee",
    ];
    assert_eq!(parse_args(input), want);
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

#[allow(unused)]
fn match_next_wowd(value: &str) -> Option<&str> {
    let value = value.trim_start();
    if let Some(pos) = value.find(|c: char| !c.is_whitespace()) {
        Some(&value[pos..])
    } else {
        None
    }
}

#[allow(unused)]
fn match_word<'a>(word: &str, from: &'a str) -> Option<&'a str> {
    let from = from.trim_start();
    if word.len() > from.len() {
        return None;
    }
    if &from[..word.len()] == word {
        Some(&from[word.len()..])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::{find_path, match_word, Cmd};

    #[test]
    fn test_match_word() {
        let from = "";
        let word = "";
        let got = match_word(word, from);
        assert_eq!(got, Some(""));
        let from = "";
        let word = "hello";
        let got = match_word(word, from);
        assert_eq!(got, None);
        let from = "hello";
        let word = "hello";
        let got = match_word(word, from);
        assert_eq!(got, Some(""));
        let from = "   hello";
        let word = "hello";
        let got = match_word(word, from);
        assert_eq!(got, Some(""));
        let from = "   hello  world";
        let word = "hello";
        let got = match_word(word, from);
        assert_eq!(got, Some("  world"));
    }
    #[test]
    fn test_find_path() {
        let got = find_path("ls");
        assert_eq!(got, Some("/usr/bin/ls".to_owned()));
        let got = find_path(
            "/home/eagle/development/codecrafters-shell-rust/target/debug/codecrafters-shell",
        );
        //assert_eq!(got, Some("/usr/bin/ls".to_owned()));
    }
    #[test]
    fn parse_cmd() {
        let input = "exit";
        let cmd = Cmd::from(input);
        assert_eq!(cmd, Cmd::Exit(0));
        let input = "exit 1";
        let cmd = Cmd::from(input);
        assert_eq!(cmd, Cmd::Exit(1));
        let input = "exit 100";
        let cmd = Cmd::from(input);
        assert_eq!(cmd, Cmd::Exit(100));
        let input = "  echo hello   world'hii'";
        let cmd = Cmd::from(input);
        assert_eq!(
            cmd,
            Cmd::Echo(vec![
                Cow::Borrowed("hello"),
                Cow::Borrowed("world"),
                Cow::Borrowed("hii")
            ])
        );
        let input = "  pwd  ";
        let cmd = Cmd::from(input);
        assert_eq!(cmd, Cmd::Pwd);
        let input = "cd   tmp/";
        let cmd = Cmd::from(input);
        assert_eq!(cmd, Cmd::Cd("tmp/".into()));
        let input = " my-cmd   tmp/ hello.txt 'arg    1'";
        let cmd = Cmd::from(input);
        assert_eq!(
            cmd,
            Cmd::Other(
                "my-cmd".into(),
                vec!["tmp/".into(), "hello.txt".into(), "arg    1".into()]
            )
        );
    }
}
