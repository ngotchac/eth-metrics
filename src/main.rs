extern crate chrono;
extern crate clap;
extern crate console;
extern crate fs_extra;
extern crate indicatif;
extern crate libc;
extern crate web3;
extern crate separator;
extern crate rand;
extern crate statrs;
extern crate tempdir;
extern crate gnuplot;
extern crate regex;

mod child_guard;
mod plotter;
mod runner;

use std::fs;
use std::path::PathBuf;
use std::io::{Error, ErrorKind};
use std::time::Instant;

use chrono::prelude::*;
use clap::{Arg, App};
use console::style;
use indicatif::HumanDuration;

use self::runner::Runner;

const NUM_RUNS: usize = 3;

fn run(bin_path: String, data_path: PathBuf, name: String, output_path: PathBuf) -> Result<(), Error> {
	if !fs::metadata(&bin_path)?.is_file() {
		return Err(Error::new(ErrorKind::Other, "The given binary path is not a file."));
	}
	if !fs::metadata(&data_path)?.is_dir() {
		return Err(Error::new(ErrorKind::Other, "The given data path is not a directory."));
	}

	let now = Local::now();
	let output_path = output_path.join(format!("{}_{}", name, now.format("%Y-%m-%dT%H:%M:%S").to_string()));
	fs::create_dir_all(&output_path)?;

    let started = Instant::now();
	let mut runner = Runner::new(bin_path, data_path, name.clone(), output_path)?;

	println!("Running metrics for {}\n", name);

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
		.arg(Arg::with_name("binary")
			.short("b")
			.long("bin")
			.value_name("BINARY")
			.help("The binary of the ETH-node to run.")
			.required(true)
			.takes_value(true))
		.arg(Arg::with_name("data")
			.short("d")
			.long("data")
			.value_name("FOLDER")
			.help("The path of the data folder to use.")
			.required(true)
			.takes_value(true))
		.arg(Arg::with_name("name")
			.short("n")
			.long("name")
			.value_name("NAME")
			.help("The name of this analysis.")
			.required(true)
			.takes_value(true))
		.arg(Arg::with_name("output")
			.short("o")
			.long("output")
			.value_name("FOLDER")
			.help("The folder where the outputs go.")
			.required(true)
			.takes_value(true))
        .get_matches();

    let bin_path = matches.value_of("binary").unwrap();
	let bin_path = String::from(bin_path);

    let data_path = matches.value_of("data").unwrap();
	let data_path = PathBuf::from(data_path).join("chains");

    let name = matches.value_of("name").unwrap();
	let name = String::from(name);

	let output_path = matches.value_of("output").unwrap();
	let output_path = PathBuf::from(output_path);

    if let Err(error) = run(bin_path, data_path, name, output_path) {
        println!("{}{}", style("error: ").bold().red(), error);
        ::std::process::exit(1);
    }
}
