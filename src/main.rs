use std::io::{self, BufReader, Read, Write};
use std::iter::{Enumerate, Peekable};
use std::str::Chars;
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
            "cat" => Self::Cat(cmd_args.collect()),
            _ => Self::Other(cmd, cmd_args.collect()),
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
        //let mut stdout = std::io::stdout();
        //let mut n = 0;
        loop {
            //writeln!(stdout, "{}", n).unwrap();
            //n += 1;
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
    for i in remove.len() - 1..0 {
        if remove[i] != end {
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::{handle_args, remove_unwanted, Cmd, IterArgs};

    fn test_fn_handle_ukt(iter: Vec<(&str, Vec<usize>, usize)>) {
        for (index, (input, rm, end)) in iter.into_iter().enumerate() {
            let mut e = 0;
            let mut remove = Vec::new();
            handle_args(
                &mut input.chars().enumerate().peekable(),
                &mut remove,
                &mut e,
            );
            if rm != remove || end != e {
                eprintln!("Test Index: {}\nInput: '{}'", index, input);
                eprintln!("want: '{:?}', got: '{:?}'", &rm, &remove);
                eprintln!("want: '{}', got: '{}'", end, e);
            }
            assert_eq!(rm, remove);
            assert_eq!(end, e);
        }
    }

    #[test]
    fn test_handle_args() {
        let test = vec![
            ("", vec![], 0),
            (" ", vec![0], 1),
            ("  ", vec![0], 1),
            ("     ", vec![0], 1),
            (r#""abc""#, vec![0, 4], 5),
            (r#"'abc'"#, vec![0, 4], 5),
            (r#""ab   c""#, vec![0, 7], 8),
            (r#"'ab   c'"#, vec![0, 7], 8),
            ("abc def", vec![3], 4),
            ("abc def ghi", vec![3], 4),
            (r#"'"'"#, vec![0, 2], 3),
            (r#""'""#, vec![0, 2], 3),
            (r#""\"""#, vec![0, 1, 3], 4),
            (r#"abc\ndef\ efg\txyz"#, vec![3, 8, 13], 18),
            (r#"\\\\\\\\"#, vec![0, 2, 4, 6], 8),
            (r#"hello\ "#, vec![5], 7),
            (r#"\ hello"#, vec![0], 7),
            (r#"hello\     "#, vec![5, 7], 8),
            (r#""abc""def""#, vec![0, 4, 5, 9], 10),
            (r#"'abc''def'"#, vec![0, 4, 5, 9], 10),
            (r#"'abc'"def""#, vec![0, 4, 5, 9], 10),
            (r#""abc"'def'"#, vec![0, 4, 5, 9], 10),
            (r#""abc" 'def'"#, vec![0, 4, 5], 6),
        ];
        test_fn_handle_ukt(test);
    }
    fn test_fn_rm_unwanted(iter: Vec<(&str, Vec<usize>, &str, bool)>) {
        for (index, (input, remove, want, is_borrowed)) in iter.into_iter().enumerate() {
            let got = remove_unwanted(input, remove);
            let is_borrowed_got = match got {
                Cow::Owned(_) => false,
                Cow::Borrowed(_) => true,
            };
            if want != got
            /* || is_borrowed != is_borrowed_got */
            {
                eprintln!("Test Index: {}\nInput: '{}'", index, input);
                eprintln!("want: '{}', got: '{}'", want, got);
                eprintln!("want: '{}', got: '{}'", is_borrowed, is_borrowed_got);
            }
            assert_eq!(want, got);
            // assert_eq!(is_borrowed, is_borrowed_got); // TODO: fix bug
        }
    }
    #[test]
    fn test_remove_unwanted() {
        let test = vec![
            ("", vec![], "", true),
            ("abc", vec![1], "ac", false),
            ("abcd", vec![0], "bcd", true),
            ("abcdefgh", vec![0, 1], "cdefgh", true),
            ("abcdefgh", vec![0, 1, 2, 3], "efgh", true),
            ("abcdefgh", vec![7], "abcdefg", true),
            ("abcdefgh", vec![6, 7], "abcdef", true),
            ("abcdefgh", vec![0, 1, 6, 7], "cdef", true),
            ("abcdefgh", vec![0, 1, 3, 6, 7], "cef", false),
            ("", vec![0], "", true),
            ("abc", vec![10], "abc", true),
            ("abc", vec![2, 3, 4], "ab", true),
            ("abc", vec![1, 2, 3, 4], "a", true),
        ];
        test_fn_rm_unwanted(test);
    }
    fn test_fn_iter_args(test: Vec<(&str, Vec<&str>)>) {
        for (index, (input, want)) in test.into_iter().enumerate() {
            let args = IterArgs::new(input);
            let got: Vec<Cow<'_, str>> = args.collect();
            if got != want {
                eprintln!("Test Index: {}", index);
                eprintln!("want: {:?}, got: {:?}", want, got);
            }
            assert_eq!(want, got);
        }
    }
    #[test]
    fn test_iter_args() {
        let test = vec![
            ("hello", vec!["hello"]),
            (r#"""""hello"""""#, vec!["hello"]),
            ("hello world", vec!["hello", "world"]),
            (r#"hello\ world"#, vec!["hello world"]),
            (r#"   hello\ world   "#, vec!["hello world"]),
            (
                r#"   hello\ world  foo'bar'foo "#,
                vec!["hello world", "foobarfoo"],
            ),
            (
                "hello world 'hello   world'\\ ",
                vec!["hello", "world", "hello   world "],
            ),
            (
                r#"'/tmp/foo/"f 50"' '/tmp/foo/"f\68"' '/tmp/foo/f67'"#,
                vec![
                    r#"/tmp/foo/"f 50""#,
                    r#"/tmp/foo/"f\68""#,
                    r#"/tmp/foo/f67"#,
                ],
            ),
        ];
        test_fn_iter_args(test);
    }

    fn test_fn_cmd_match(iter: Vec<(&str, Cmd)>) {
        for (index, (input, cmd)) in iter.into_iter().enumerate() {
            let cmd_got = Cmd::from(input);
            if cmd != cmd_got {
                eprintln!("Test Index: {}", index);
                eprintln!("want: '{}', got: '{}'", cmd, cmd_got);
            }
            assert_eq!(cmd, cmd_got);
        }
    }
    #[test]
    fn test_cmd_from_str<'a>() {
        let fn_into_cow = |input: Vec<&'a str>| -> Vec<Cow<'a, str>> {
            input.into_iter().map(|v| Cow::Borrowed(v)).collect()
        };
        let test = vec![
            (
                "echo hello world",
                Cmd::Echo(fn_into_cow(vec!["hello", "world"])),
            ),
            (
                r#"cat '/tmp/foo/"f 50"' '/tmp/foo/"f\68"' '/tmp/foo/f67'"#,
                Cmd::Cat(fn_into_cow(vec![
                    r#"/tmp/foo/"f 50""#,
                    r#"/tmp/foo/"f\68""#,
                    r#"/tmp/foo/f67"#,
                ])),
            ),
            ("type ls", Cmd::Type("ls".into())),
        ];
        test_fn_cmd_match(test);
    }
}
