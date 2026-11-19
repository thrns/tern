use std::env;
use std::process::ExitCode;

const USAGE: &str = "usage: xtask <build-core|bundle|membench|gen-corpus>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some(command) => {
            eprintln!("xtask: unknown command '{command}'");
            eprintln!("{USAGE}");
        }
        None => eprintln!("{USAGE}"),
    }
    ExitCode::from(2)
}
