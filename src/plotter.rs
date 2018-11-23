use std::path::PathBuf;

use gnuplot::*;

pub type Line = (Vec<f64>, Vec<f64>);

const COLORS: &'static [&'static str] = &[
	"#5899DA",
	"#E8743B",
	"#19A979",
	"#ED4A7B",
];

struct PlotParams {
	filepath: String,
	title: String,
	y_label: String,
	y_min: f64,
	y_max: f64,
}

pub struct Plotter {
	name: String,
	output_path: PathBuf,
}

impl Plotter {
	pub fn new(name: String, output_path: PathBuf) -> Self {
		Plotter {
			name, output_path,
		}
	}

	pub fn block_height(&self, lines: &Vec<Line>) {
		let params = PlotParams {
			filepath: String::from("block_heights.png"),
			title: String::from("Block heights"),
			y_label: String::from("Block Height"),
			y_min: 0.0,
			y_max: 500_000.0,
		};

		self.plot(params, lines);
	}

	pub fn block_speeds(&self, lines: &Vec<Line>) {
		let params = PlotParams {
			filepath: String::from("block_speeds.png"),
			title: String::from("Block speed"),
			y_label: String::from("Block speed (bps)"),
			y_min: 0.0,
			y_max: 6_000.0,
		};

		self.plot(params, lines);
	}

	pub fn peer_count(&self, lines: &Vec<Line>) {
		let params = PlotParams {
			filepath: String::from("peer_counts.png"),
			title: String::from("Peer count"),
			y_label: String::from("Number of peers"),
			y_min: 0.0,
			y_max: 100.0,
		};

		self.plot(params, lines);
	}

	fn plot(&self, params: PlotParams, lines: &Vec<Line>) {
		let mut fg = Figure::new();

		let title = format!("{} for {}", params.title, self.name);

		{
			let fg_2d = fg.axes2d()
				.set_title(title.as_str(), &[])
				.set_x_label("Time (s)", &[])
				.set_x_range(Fix(0.0), Auto)
				.set_y_label(params.y_label.as_str(), &[])
				.set_y_range(Fix(params.y_min), Fix(params.y_max))
				.set_y_ticks(Some((Auto, 1)), &[Format("%'.0f")], &[]);

			let mut index = 1;
			for (times, data) in lines {
				let caption = format!("Run #{}", index);
				let color = COLORS[(index - 1) % COLORS.len()];
				fg_2d.lines(times, data, &[Caption(&caption), LineWidth(1.5), Color(color)]);
				index += 1;
			}
		}

		let filepath = self.output_path.join(params.filepath);

		fg.set_terminal("pngcairo size 1366, 768", filepath.to_str().unwrap());
		fg.show();
	}
}
