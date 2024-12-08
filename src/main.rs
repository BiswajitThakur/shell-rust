use std::borrow::Cow;
#[allow(unused_imports)]
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let stdin = io::stdin();
    write!(stdout, "$ ")?;
    stdout.flush()?;

    for line in stdin.lines() {
        let line = line?;
        let cmd = Cmd::new(line.trim());
        cmd.execute(&mut stdout)?;
        write!(stdout, "$ ")?;
        stdout.flush()?;
    }
    Ok(())
}

struct Cmd<'a> {
    cmd: Cow<'a, str>,
    args: Vec<Cow<'a, str>>,
}

impl<'a> Cmd<'a> {
    fn new<T: Into<Cow<'a, str>>>(value: T) -> Self {
        let cmd: Cow<'a, str> = value.into();
        let mut iter = cmd.split_whitespace();
        Self {
            cmd: Cow::Owned(iter.next().unwrap().to_owned()),
            args: iter.map(|v| Cow::Owned(v.to_owned())).collect(),
        }
    }
    fn execute<W: io::Write>(&self, stdout: &mut W) -> io::Result<()> {
        match self.cmd.as_ref() {
            "exit" => std::process::exit(
                self.args
                    .get(0)
                    .unwrap_or(&Cow::Borrowed("0"))
                    .parse()
                    .unwrap(),
            ),
            _ => {}
        }
        writeln!(stdout, "{}: not found", self.cmd.trim())
    }
}
