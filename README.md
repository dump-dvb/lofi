# R09 Location Finder

Nifty multitool for your R09 transmission location needs.

## Usage

For more info try passing `--help` to `lofi` or any subcommand.

Tool can `correlate` telegrams to GPS trackpoints, `filter` only relevant telegrams, `merge` several `stops.json` formatted files and rearange them in conventional file structure and convert `stops2geo`, which will produce a geojson file that is useful for visualisation.

## Examples

- `lofi correltate -t filtered_telegrams -t filtered_telegrams2 -g gpx -g gpx2 -o stops.json -G geo.json`
- `lofi fiter -w wartrammer.json -w wartrammer2.json -t telegrams1.csv -t telegrams2.csv -o filtered-telegrams.csv`
- `lofi stops2geo -i stops.json -o geo.json`
- `lofi merge -o ./output-folder ./stops1.json ./stops3.json ./stops2.json`

