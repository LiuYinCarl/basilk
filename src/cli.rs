pub struct Cli;

/// What the program should do based on its command-line arguments.
#[derive(Debug, PartialEq, Eq)]
pub enum CliAction {
    Run,
    ShowVersion,
}

impl Cli {
    /// Decide what to do from the command-line arguments (without the
    /// program name, which is always the first argument).
    pub fn parse(mut args: impl Iterator<Item = String>) -> CliAction {
        match args.next() {
            Some(arg) if arg == "--version" => CliAction::ShowVersion,
            _ => CliAction::Run,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_means_run() {
        assert_eq!(Cli::parse(std::iter::empty()), CliAction::Run);
    }

    #[test]
    fn version_flag_is_recognized() {
        assert_eq!(
            Cli::parse(["--version".to_string()].into_iter()),
            CliAction::ShowVersion
        );
    }

    #[test]
    fn unknown_arguments_are_ignored() {
        assert_eq!(
            Cli::parse(["--help".to_string()].into_iter()),
            CliAction::Run
        );
        assert_eq!(
            Cli::parse(["basilk".to_string()].into_iter()),
            CliAction::Run
        );
        // Only the first argument counts
        assert_eq!(
            Cli::parse(["basilk".to_string(), "--version".to_string()].into_iter()),
            CliAction::Run
        );
    }
}
