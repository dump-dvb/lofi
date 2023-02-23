mod correlate;
mod filter;
mod gps;
mod types;

use crate::correlate::correlate;
use crate::filter::filter;
use crate::gps::Gps;
use crate::types::R09Iter;

use tlms::locations::LocationsJson;
use tlms::telegrams::r09::R09SaveTelegram;

use std::fs::{write, File};

use clap::{Args, Parser, Subcommand};
use geojson::{Feature, FeatureCollection, Geometry, JsonObject, JsonValue, Value};
use log::info;

// Clap sturcts
#[derive(Parser, Debug)]
#[clap(name = "R09 Location Finder")]
#[clap(author = "Dump DVB Institute <dump@dvb.solutions>")]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(about = "R09 telegram transmission location data multitool", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
    /// Verbose output, can be passed more than once
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Correlate R09 Telegrams to the GPS data
    Correlate(CorrelateArgs),
    /// Merge the different stops.json-formatted files and produce windhsield-ready output
    Merge(MergeArgs),
    /// Convert stops.json to a geojson file, useful for visualizing/debugging
    #[clap(name = "stops2geo")]
    StopsToGeo(StopsToGeoArgs),
    /// Filter the telegrams using measurement intervals from wartrammer-40k
    Filter(FilterArgs),
}

#[derive(Args, Debug)]
struct CorrelateArgs {
    /// telegram CSV file
    #[clap(short, long)]
    telegrams: Vec<String>,
    /// GPX-formatted gps track
    #[clap(short, long, required = true)]
    gps: Vec<String>,
    /// Legacy format gps data, you most probably don't need that
    #[clap(long)]
    gps_legacy: Vec<String>,
    /// wartrammer-40k json file with measured public transport runs
    #[clap(short, long)]
    wartrammer: Option<Vec<String>>,
    /// JSON outut file in stop-names format, if not specified result is printed on stdout
    #[clap(short, long)]
    stops_json: String,
    /// Geojson output for diagnostics
    #[clap(short = 'j', long)]
    geojson: Option<String>,
    /// Maximum time difference in seconds between gps point and telegram transmission time. Bigger
    /// values result in more transmission position matched at the cost of accuracy.
    #[clap(long, default_value = "5")]
    corr_window: i64,
}

#[derive(Args, Debug)]
struct MergeArgs {
    /// output directory in which stops.json-formatted files will be written
    #[clap(short, long, required = true)]
    out_dir: String,
    /// Input stops.json-formatted files
    #[clap(required = true)]
    stops: Vec<String>,
}

#[derive(Args, Debug)]
struct StopsToGeoArgs {
    /// Input stops.json files
    #[clap(required = true)]
    stops: Vec<String>,
    /// geojson file to write, if not specified geojson is dumped to stdout
    #[clap(short = 'o', long = "geojson")]
    geojson: Option<String>,
}

#[derive(Args, Debug)]
struct FilterArgs {
    /// csv with R09 telegrams
    #[clap(short, long, required = true)]
    telegrams: Vec<String>,
    /// wartrammer json output
    #[clap(short, long, required = true)]
    wartrammer: Vec<String>,
    /// output csv file to write.
    #[clap(short, long, required = true)]
    outfile: String,
}

fn main() {
    let cli = Cli::parse();
    // set verbosity level
    println!("{:?}", cli.verbose);

    // run subcommand
    match cli.command {
        Command::Correlate(opts) => correlate_cmd(opts),
        Command::Merge(opts) => merge(opts),
        Command::StopsToGeo(opts) => stops2geo(opts),
        Command::Filter(opts) => filter_cmd(opts),
    }
}

// Handles `lofi correlate`
fn correlate_cmd(cli: CorrelateArgs) {
    info!("got args: {:?}", cli);
    let telegrams = match cli.wartrammer {
        Some(wt) => filter::filter(read_telegrams(cli.telegrams), wt),
        None => read_telegrams(cli.telegrams),
    };

    let mut gps: Gps = Gps::empty();
    for filepath in cli.gps {
        gps.insert_from_gpx_file(&filepath);
    }
    for filepath in cli.gps_legacy {
        gps.insert_from_legacy(&filepath);
    }

    let stops = correlate(telegrams, gps, cli.corr_window);

    stops.write(&cli.stops_json);

    if let Some(path) = cli.geojson {
        let features = get_features(&stops);
        let feature_collection = FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        };
        let geojson_string = feature_collection.to_string();

        write(path, geojson_string).expect("Couldn't write geojson");
    };
}

// handles `lofi filter`
fn filter_cmd(opts: FilterArgs) {
    let tg = read_telegrams(opts.telegrams);
    let filtered = filter(tg, opts.wartrammer);
    let outfile = File::create(opts.outfile).expect("Couldn't create output file");
    let mut writer = csv::Writer::from_writer(outfile);
    filtered
        .filter_map(|t| writer.serialize(t).ok())
        .for_each(drop);
}

fn merge(_opts: MergeArgs) {
    todo!();
}

/// Convert the json-formatted locations to geojson, useful for debug
fn stops2geo(opts: StopsToGeoArgs) {
    let mut features: Vec<Feature> = vec![];
    for path in opts.stops {
        let stops = LocationsJson::from_file(&path).expect("Couldn't deserialize stops file");
        features.append(&mut get_features(&stops));
    }

    let feature_collection = FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    };
    let geojson_string = feature_collection.to_string();

    match opts.geojson {
        Some(path) => {
            write(path, geojson_string).expect("Couldn't write geojson");
        }
        None => {
            println!("{geojson_string}");
        }
    }
}

fn read_telegrams(paths: Vec<String>) -> R09Iter {
    Box::new(
        paths
            .into_iter()
            .map(|p| File::open(p).expect("couldn't open file"))
            .map(csv::Reader::from_reader)
            .flat_map(|r| r.into_deserialize())
            // TODO proper Result<Option<_>,_> handling
            .map(|tg| tg.ok().unwrap()),
    )
}

fn get_features(locs: &LocationsJson) -> Vec<Feature> {
    let mut features: Vec<Feature> = vec![];
    for (_reg, v) in locs.data.iter() {
        for (mp, loc) in v {
            let mut properties = JsonObject::new();
            let propval = format!("{mp}");
            properties.insert("name".to_string(), JsonValue::from(propval));
            features.push(Feature {
                bbox: None,
                geometry: Some(Geometry::new(Value::Point(vec![loc.lon, loc.lat]))),
                id: None,
                properties: Some(properties),
                foreign_members: None,
            })
        }
    }

    features
}
