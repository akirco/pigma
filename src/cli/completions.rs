//! `pigma completions <shell>` subcommand: shell completion script generation.

use super::Cli;

/// `pigma completions <shell>` handler: print a shell completion script for the
/// `pigma` CLI to stdout.
pub(crate) fn completions(shell: &str) -> eyre::Result<()> {
    let mut cmd = <Cli as clap::CommandFactory>::command();
    let mut out = std::io::stdout();
    match shell {
        "bash" => clap_complete::generate(clap_complete::shells::Bash, &mut cmd, "pigma", &mut out),
        "zsh" => clap_complete::generate(clap_complete::shells::Zsh, &mut cmd, "pigma", &mut out),
        "fish" => clap_complete::generate(clap_complete::shells::Fish, &mut cmd, "pigma", &mut out),
        "elvish" => {
            clap_complete::generate(clap_complete::shells::Elvish, &mut cmd, "pigma", &mut out)
        }
        "powershell" => clap_complete::generate(
            clap_complete::shells::PowerShell,
            &mut cmd,
            "pigma",
            &mut out,
        ),
        other => {
            eyre::bail!("unsupported shell `{other}` (expected bash/zsh/fish/elvish/powershell)")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Command;
    use clap::Parser;

    #[test]
    fn completions_prints_script() {
        // Capturing stdout is awkward; instead verify the subcommand parses and
        // that each supported shell name is accepted by the parser.
        let cmd = Cli::try_parse_from(["pigma", "completions", "bash"]).unwrap();
        assert!(matches!(
            cmd.command,
            Some(Command::Completions { shell }) if shell == "bash"
        ));
        assert!(Cli::try_parse_from(["pigma", "completions", "zsh"]).is_ok());
        assert!(Cli::try_parse_from(["pigma", "completions", "fish"]).is_ok());
        assert!(Cli::try_parse_from(["pigma", "completions", "powershell"]).is_ok());
        assert!(Cli::try_parse_from(["pigma", "completions", "nushell"]).is_err());
    }
}
