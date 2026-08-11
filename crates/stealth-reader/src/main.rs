use clap::Parser;
use stealth_reader::Cli;

fn main() {
    let cli = Cli::parse();

    // The composition root is intentionally established before adapters are
    // introduced. The first vertical slice will replace this status with the
    // real library/startup flow.
    let intent = if cli.resume { "resume" } else { "library" };
    println!("cli-stealth-reader-v2: startup intent={intent}");
}
