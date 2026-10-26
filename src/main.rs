//! The `suzu` command.

use std::process::ExitCode;

const USAGE: &str = "\
usage: suzu <command> [args]

commands:
  run <file.sz>     run a program with the interpreter
  build <file.sz>   compile to native code (Cranelift)
  test              run tests
  new <name>        create a new project
  help              print this message
  version           print the version
";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run(String),
    Build(String),
    Test,
    New(String),
    Help,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
enum CliError {
    MissingArg(&'static str, &'static str),
    Unknown(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::MissingArg(cmd, what) => write!(f, "`suzu {cmd}` requires {what}"),
            CliError::Unknown(cmd) => write!(f, "unknown command `{cmd}`"),
        }
    }
}

fn parse_args(args: &[String]) -> Result<Command, CliError> {
    let Some(cmd) = args.first() else {
        return Ok(Command::Help);
    };
    let operand = |name: &'static str, what: &'static str| {
        args.get(1).cloned().ok_or(CliError::MissingArg(name, what))
    };
    match cmd.as_str() {
        "run" => operand("run", "a file name").map(Command::Run),
        "build" => operand("build", "a file name").map(Command::Build),
        "test" => Ok(Command::Test),
        "new" => operand("new", "a project name").map(Command::New),
        "help" | "-h" | "--help" => Ok(Command::Help),
        "version" | "-V" | "--version" => Ok(Command::Version),
        other => Err(CliError::Unknown(other.to_string())),
    }
}

fn not_yet(what: &str, milestone: &str) -> ExitCode {
    eprintln!("suzu: `{what}` is not implemented yet (planned for {milestone})");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Command::Run(_)) => not_yet("run", "M4"),
        Ok(Command::Build(_)) => not_yet("build", "M6"),
        Ok(Command::Test) => not_yet("test", "M4"),
        Ok(Command::New(_)) => not_yet("new", "phase 2"),
        Ok(Command::Help) => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Version) => {
            println!("suzu {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("suzu: {e}\n\n{USAGE}");
            ExitCode::from(64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, CliError> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse_args(&args)
    }

    #[test]
    fn no_args_shows_help() {
        assert_eq!(parse(&[]), Ok(Command::Help));
    }

    #[test]
    fn run_takes_a_file() {
        assert_eq!(
            parse(&["run", "examples/01_fizzbuzz.sz"]),
            Ok(Command::Run("examples/01_fizzbuzz.sz".into()))
        );
        assert_eq!(
            parse(&["run"]),
            Err(CliError::MissingArg("run", "a file name"))
        );
    }

    #[test]
    fn unknown_command_is_an_error() {
        assert_eq!(parse(&["fly"]), Err(CliError::Unknown("fly".into())));
    }
}
