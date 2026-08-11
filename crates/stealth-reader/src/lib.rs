use clap::Parser;

/// Full-screen terminal reader for EPUB, CBZ, and PDF books.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Resume the most recently opened book.
    #[arg(long)]
    pub resume: bool,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::Cli;

    #[test]
    fn resume_flag_matches_the_v1_entry_point() {
        assert!(
            Cli::try_parse_from(["stealth-reader", "--resume"])
                .expect("--resume should be accepted")
                .resume
        );
        assert!(
            !Cli::try_parse_from(["stealth-reader"])
                .expect("no arguments should be accepted")
                .resume
        );
    }

    #[test]
    fn unknown_flags_are_rejected() {
        assert!(Cli::try_parse_from(["stealth-reader", "--unknown"]).is_err());
    }
}
