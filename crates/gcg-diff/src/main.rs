//! `gcg-diff`: the differential runner and gate tool (PLAN §8.7, §9.9-§9.10).

fn main() -> std::process::ExitCode {
    gcg_diff::cli::main(std::env::args().skip(1).collect())
}
