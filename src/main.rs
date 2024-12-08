use std::borrow::Cow;
#[allow(unused_imports)]
use std::io::{self, Write};

fn main() -> io::Result<()> {
    // Uncomment this block to pass the first stage
    print!("$ ");
    io::stdout().flush()?;

    // Wait for user input
    let stdin = io::stdin();
    let mut input = String::new();
    stdin.read_line(&mut input)?;
    let mut stdout = io::stdout().lock();
    let cmd = Cmd::new(input);
    cmd.execute(&mut stdout)?;
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
