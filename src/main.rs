extern crate clap;
extern crate console;
extern crate indicatif;
extern crate libc;
extern crate web3;
extern crate separator;
extern crate statrs;
extern crate tempdir;
extern crate gnuplot;

mod child_guard;
mod plotter;
mod runner;

use std::fs;
use std::io;
use std::time::Instant;

use clap::{Arg, App};
use console::style;
use indicatif::HumanDuration;

use self::runner::Runner;

const NUM_RUNS: usize = 2;

fn run(bin_path: String) -> Result<(), io::Error> {
    let started = Instant::now();
	let mut runner = Runner::new(bin_path)?;

	for run_idx in 0..NUM_RUNS {
		println!(
			"{} Starting the node for run #{}...",
			style("[1/4]").bold().dim(), run_idx + 1
		);
		runner.start()?;

		println!(
			"{} Waiting for the node to be ready...",
			style("[2/4]").bold().dim()
		);
		runner.wait_until_ready()?;

		println!(
			"{} Collecting data...",
			style("[3/4]").bold().dim()
		);
		runner.collect_data()?;

		println!(
			"{} Stopping the node...",
			style("[4/4]").bold().dim()
		);

		runner.stop()?;
		println!("");
	}

	runner.analyse()?;
	runner.plot()?;

    println!("✨ Done in {}", HumanDuration(started.elapsed()));
	Ok(())
}

fn main() {
    let matches = App::new("Eth-Metrics")
        .version("0.1")
        .author("Nicolas Gotchac <ngotchac@gmail.com>")
        .about("Run an ETH-node and collect some metrics.")
        .arg(Arg::with_name("BIN_PATH")
            .help("The binary of the ETH-node to run.")
            .required(true)
            .index(1))
        .get_matches();

    let bin_path = matches.value_of("BIN_PATH").unwrap();

    match fs::metadata(bin_path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                println!("Error: the given path is not a file.");
                return;
            }
        },
        Err(err) => {
            println!("Error: {}", err);
            return;
        },
    }

    if let Err(error) = run(String::from(bin_path)) {
        println!("{}{}", style("error: ").bold().red(), error);
        ::std::process::exit(1);
    }
}
