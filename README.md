# Eth-Metrics

This is a script to gather some metrics for a given Ethereum Node binary.
Currently it only gathers peer count and the block height, but this can
be easily extended.

It runs the binary for some time, and creates graphs for the gathered
metrics

## Usage

```bash
eth-metrics --bin <BINARY> --data <FOLDER> --name <NAME> --output <FOLDER>
```

The `--data` folder should be a Parity data folder, thus containing a `chains` directory.
The idea is that this would be the starting point of all the runs, thus having this folder
containing DB for a few thousands blocks (we had some issue with node sending wrong
first blocks on Foundation, thus skipping the first few thousands blocks makes more reliable
runs).
