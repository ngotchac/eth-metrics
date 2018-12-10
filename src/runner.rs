use std::error::Error as StdError;
use std::net::TcpListener;
use std::fs::{self, File};
use std::io::{Error, ErrorKind, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};

use rand::{thread_rng, Rng};
use rand::distributions::Uniform;
use fs_extra::dir::{self, CopyOptions};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use separator::Separatable;
use statrs::statistics::{Min, Max, Mean, Variance};
use tempdir::TempDir;
use web3::{futures::Future, Web3, transports::Http as HttpTransport, transports::EventLoopHandle};

use child_guard::ChildGuard;
use plotter::{Plotter, Line};

const ANALYSIS_TIME_SKIP: Duration = Duration::from_secs(60 * 5);
const BLOCK_SPEEDS_AVERAGE_DURATION: Duration = Duration::from_secs(10);
const DATA_COLLECTION_DURATION: Duration = Duration::from_secs(60 * 10);
const DATA_COLLECTION_INTERVAL: Duration = Duration::from_millis(500);
const MIN_PEERS: u32 = 75;

fn duration_as_f64(duration: Duration) -> f64 {
    duration.as_secs() as f64 + duration.subsec_millis() as f64 / 1_000.0
}

fn duration_to_ms(duration: Duration) -> u64 {
	duration.as_secs() * 1_000 + duration.subsec_millis() as u64
}

fn get_available_ports() -> Vec<u16> {
	let mut rng = thread_rng();

	rng.sample_iter(&Uniform::new_inclusive(8_000, 9_000))
        .filter(|port| port_is_available(*port))
		.take(2)
		.collect()
}

fn port_is_available(port: u16) -> bool {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub struct Runner {
	bin_path: String,
	data_path: PathBuf,
	output_path: PathBuf,
	name: String,
	version: String,
	tmp_dir: Option<TempDir>,
	child: Option<ChildGuard>,
	web3: Option<(Web3<HttpTransport>, EventLoopHandle)>,
	block_heights: Vec<Line>,
	block_speeds: Vec<Line>,
	peer_counts: Vec<Line>,
}

impl Runner {
	/// Creates a new runner with the given binary path
	pub fn new(bin_path: String, data_path: PathBuf, name: String, output_path: PathBuf) -> Result<Self, Error> {
		let version = Runner::version(&bin_path)?;

		Ok(Runner {
			bin_path,
			data_path,
			output_path,
			name,
			version,
			tmp_dir: None,
			child: None,
			web3: None,
			block_heights: Vec::new(),
			block_speeds: Vec::new(),
			peer_counts: Vec::new(),
		})
	}

	/// Get the version of the given binary
	fn version(bin_path: &String) -> Result<String, Error> {
		let output = Command::new(bin_path)
			.arg("--version")
			.output()?;

		let re = Regex::new(r"version (?P<version>[^\s]+)").unwrap();
		let stdout = String::from_utf8_lossy(&output.stdout);
		let captures = re.captures(&stdout);
		let version = match captures {
			Some(ref captures) => &captures["version"],
			_ => return Err(Error::new(ErrorKind::Other, "Could not find version of the binary.")),
		};

		Ok(String::from(version))
	}

	/// Start the node with the pre-defined configuration
	pub fn start(&mut self) -> Result<(), Error> {
		let tmp_dir = TempDir::new("eth-metrics")?;
		let tmp_data_dir_path = tmp_dir.path().join("parity-data");
		let tmp_data_dir = match tmp_data_dir_path.to_str() {
			Some(tmp_data_dir) => tmp_data_dir,
			None => return Err(Error::new(ErrorKind::Other, "Could not find the node's data directory.")),
		};

		fs::create_dir_all(&tmp_data_dir)?;
		let copy_options = CopyOptions::new();
		match dir::copy(&self.data_path, tmp_data_dir, &copy_options) {
			Err(e) => return Err(Error::new(ErrorKind::Other, format!("Could not copy the data directory: {}", e.description()))),
			_ => (),
		}

		let ports = get_available_ports();
		if ports.len() < 2 {
			return Err(Error::new(ErrorKind::Other, "Could not find any available port."));
		}
		let port = ports[0];
		let rpc_port = ports[1];
		let child = Command::new(&self.bin_path)
			.arg("-d").arg(tmp_data_dir)
			.arg("--chain").arg("foundation")
			.arg("--min-peers").arg(MIN_PEERS.to_string())
			.arg("--port").arg(port.to_string())
			.arg("--jsonrpc-port").arg(rpc_port.to_string())
			.arg("--no-warp")
			.arg("--no-ws")
			.arg("--no-ipc")
			.arg("--no-secretstore")
			.stderr(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()?;

    	let child_guard = ChildGuard::new(child);
		let (_eloop, transport) = HttpTransport::new(&format!("http://localhost:{}", rpc_port)).unwrap();
        let web3 = Web3::new(transport);

		self.child = Some(child_guard);
		self.web3 = Some((web3, _eloop));
		self.tmp_dir = Some(tmp_dir);

		Ok(())
	}

	pub fn stop(&mut self) -> Result<(), Error> {
		self.web3 = None;

		{
			let child = match self.child {
				Some(ref mut child) => child,
				None => return Err(Error::new(ErrorKind::Other, "The Runner has not been started yet.")),
			};

			child.terminate();
		}

		self.child = None;

		let tmp_dir = ::std::mem::replace(&mut self.tmp_dir, None);
		tmp_dir.map_or(Ok(()), |dir| dir.close())?;

		Ok(())
	}

	/// Wait until the node is ready to be queried
	pub fn wait_until_ready(&mut self) -> Result<(), Error> {
		let web3 = match self.web3 {
			Some((ref web3, _)) => web3,
			None => return Err(Error::new(ErrorKind::Other, "The Runner has not been started yet.")),
		};

		let timedout = Arc::new(AtomicBool::from(false));
		let timedout_2 = timedout.clone();

		thread::spawn(move || {
			thread::sleep(Duration::from_secs(5));
			timedout_2.store(true, Ordering::SeqCst);
		});

        loop {
			match self.child {
				Some(ref mut child) => child.check()?,
				_ => (),
			}
			if timedout.load(Ordering::SeqCst) {
				return Err(Error::new(ErrorKind::Other, "Node was node ready even after 5s."));
			}
            match web3.eth().block_number().wait() {
                Ok(_) => {
                    break;
                },
                Err(_e) => {
					// println!("Error: {}", e);
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }

		Ok(())
	}

	/// Collect some data for some time
	pub fn collect_data(&mut self) -> Result<(), Error> {
		let web3 = match self.web3 {
			Some((ref web3, _)) => web3,
			None => return Err(Error::new(ErrorKind::Other, "The Runner has not been started yet.")),
		};

        let pb = ProgressBar::new(duration_to_ms(DATA_COLLECTION_DURATION));
        let spinner_style = ProgressStyle::default_bar()
            .template("{spinner:.green} {bar:40.cyan/blue} {msg} ({eta})");
        pb.set_style(spinner_style);

        let start = Instant::now();
		let mut elapsed = Duration::new(0, 0);

		let mut times = Vec::new();
		let mut block_heights = Vec::new();
		let mut peer_counts = Vec::new();

        while elapsed < DATA_COLLECTION_DURATION {
			match self.child {
				Some(ref mut child) => child.check()?,
				_ => (),
			}
            let block_number = match web3.eth().block_number().wait() {
				Ok(block_number) => block_number,
				Err(_) => return Err(Error::new(ErrorKind::Other, "Could not fetch block number.")),
			};
            let peer_count = match web3.net().peer_count().wait() {
				Ok(peer_count) => peer_count,
				Err(_) => return Err(Error::new(ErrorKind::Other, "Could not fetch peer count.")),
			};

			times.push(duration_as_f64(elapsed));
			block_heights.push(block_number.as_u32() as f64);
			peer_counts.push(peer_count.as_u32() as f64);

            pb.set_position(duration_to_ms(elapsed));
            pb.set_message(format!(
				"[#{} ; {:2}/{}]",
				block_number.as_u64().separated_string(), peer_count,
				MIN_PEERS
			).as_str());

            thread::sleep(DATA_COLLECTION_INTERVAL);
			elapsed = Instant::now().duration_since(start);
        }

		let block_speeds_line = {
			// Take the average of block speeds every BLOCK_SPEEDS_AVERAGE_DURATION
			//  seconds (last element of `times`
			// is the duration of the collect)
			let avg_secs = BLOCK_SPEEDS_AVERAGE_DURATION.as_secs() as f64;
			let duration = times[times.len() - 1];
			let avg_factor = (times.len() as f64 / duration * avg_secs) as usize;

			let mut block_speeds = Vec::new();
			let mut block_speeds_times = Vec::new();

			block_speeds.push(0.0);
			block_speeds_times.push(0.0);

			for index in 1..((times.len() - 1) / avg_factor) {
				let cur_index = index * avg_factor;
				let prev_index = (index - 1) * avg_factor;

				let elapsed = times[cur_index] - times[prev_index];
				let block_count = block_heights[cur_index] - block_heights[prev_index];
				let time = times[cur_index];

				block_speeds.push(block_count / elapsed);
				block_speeds_times.push(time);
			}

			(block_speeds_times, block_speeds)
		};

		self.block_heights.push((times.clone(), block_heights));
		self.block_speeds.push(block_speeds_line);
		self.peer_counts.push((times.clone(), peer_counts));

		Ok(())
	}

	pub fn analyse(&self) -> Result<(), Error> {
		if self.block_heights.len() == 0 {
			return Err(Error::new(ErrorKind::Other, "No data have been collected."));
		}

		let mut result = String::new();

		result.push_str("Analysis results:\n");
		result.push_str(&format!("  - Version: {}\n\n", self.version));

		let skip_index = (duration_as_f64(ANALYSIS_TIME_SKIP) / duration_as_f64(DATA_COLLECTION_INTERVAL)) as usize;
		let mut index = 1;
		for (_times, peer_count) in self.peer_counts.iter() {
			let min = peer_count[skip_index..].min();
			let max = peer_count[skip_index..].max();
			let mean = peer_count[skip_index..].mean();
			let std_dev = peer_count[skip_index..].std_dev();

			result.push_str(&format!(
				"  - [Peer Count] Run #{}: min={:.0} ; max={:.0} ; mean={:.2} ; std_dev={:.2}\n",
				index, min, max, mean, std_dev));
			index += 1;
		}
		result.push_str("\n");

		// Block speeds are averaged every BLOCK_SPEEDS_AVERAGE_DURATION second
		let skip_index = (ANALYSIS_TIME_SKIP.as_secs() / BLOCK_SPEEDS_AVERAGE_DURATION.as_secs()) as usize;
		let mut index = 1;
		for (_times, block_speeds) in self.block_speeds.iter() {
			let mean = block_speeds[skip_index..].mean();
			let std_dev = block_speeds[skip_index..].std_dev();
			let max = self.block_heights[index - 1].1[self.block_heights[index - 1].1.len() - 1];

			result.push_str(&format!(
				"  - [Block Height] Run #{}: max={:.0} ; mean_speed={:.2}bps ; std_dev={:.2}\n",
				index, max, mean, std_dev));
			index += 1;
		}
		result.push_str("\n");

		let filepath = self.output_path.join("results.md");
		let mut file = File::create(filepath)?;
		write!(file, "{}", result);
		println!("{}", result);

		Ok(())
	}

	/// Plot the previously collected data
	pub fn plot(&self) -> Result<(), Error> {
		if self.block_heights.len() == 0 {
			return Err(Error::new(ErrorKind::Other, "No data have been collected."));
		}

		let plotter = Plotter::new(self.name.clone(), self.output_path.clone());

		plotter.block_height(&self.block_heights);
		plotter.block_speeds(&self.block_speeds);
		plotter.peer_count(&self.peer_counts);

		Ok(())
	}
}

impl Drop for Runner {
	fn drop(&mut self) {
		let tmp_dir = ::std::mem::replace(&mut self.tmp_dir, None);
		tmp_dir.map_or(Ok(()), |dir| dir.close()).unwrap();
	}
}
