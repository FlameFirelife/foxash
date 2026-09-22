use std::path::PathBuf;

pub enum Command {
    Run(PathBuf),
    Check(PathBuf),
    Help,
    Version,
}

pub fn parse_arguments(arguments: &[String]) -> Result<Command, String> {
    match arguments {
        [] => Err("missing command".to_string()),

        [argument] if argument == "help" || argument == "--help" || argument == "-h" => {
            Ok(Command::Help)
        }

        [argument] if argument == "version" || argument == "--version" || argument == "-V" => {
            Ok(Command::Version)
        }

        [command, file] if command == "run" => Ok(Command::Run(PathBuf::from(file))),

        [command, file] if command == "check" => Ok(Command::Check(PathBuf::from(file))),

        // Temporary backwards compatibility:
        [file] if file.ends_with(".foxash") => Ok(Command::Run(PathBuf::from(file))),

        _ => Err("usage: foxash run <file.foxash> | foxash check <file.foxash>".to_string()),
    }
}
