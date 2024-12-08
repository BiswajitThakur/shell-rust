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
}

impl<'a> Cmd<'a> {
    fn new<T: Into<Cow<'a, str>>>(value: T) -> Self {
        Self { cmd: value.into() }
    }
    fn execute<W: io::Write>(&self, stdout: &mut W) -> io::Result<()> {
        writeln!(stdout, "{}: not found", self.cmd.trim())
    }
}
