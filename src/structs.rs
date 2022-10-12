use clap::{Args, Parser, Subcommand};

use dump_dvb::locations::ReportLocation;
use dump_dvb::telegrams::r09::R09SaveTelegram;

use crate::gps::GpsPoint;

/// time difference is calculated as telegram.timestamp - gpspoint.timestamp
#[derive(Debug)]
pub struct CorrTelegram {
    pub transmission_position: i32,
    timestamp: i64,
    location_before: GpsPoint,
    location_after: GpsPoint,
}

impl CorrTelegram {
    pub fn new(tg: R09SaveTelegram, before: GpsPoint, after: GpsPoint) -> CorrTelegram {
        CorrTelegram {
            transmission_position: tg.reporting_point,
            timestamp: tg.time.timestamp(),
            location_before: before,
            location_after: after,
        }
    }

    pub fn interpolate_position(&self) -> (i32, ReportLocation) {
        (
            self.transmission_position,
            ReportLocation {
                lat: self.location_before.lat
                    + (self.timestamp - self.location_before.timestamp) as f64
                        / (self.location_after.timestamp + self.location_before.timestamp) as f64
                        * (self.location_after.lat - self.location_before.lat),
                lon: self.location_before.lon
                    + (self.timestamp - self.location_before.timestamp) as f64
                        / (self.location_after.timestamp + self.location_before.timestamp) as f64
                        * (self.location_after.lon - self.location_before.lon),
                properties: serde_json::Value::Null,
            },
        )
    }
}

#[derive(Parser, Debug)]
#[clap(name = "R09 Location Finder")]
#[clap(author = "Dump DVB Institute <dump@dvb.solutions>")]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(about = "R09 telegram transmission location data multitool", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
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
pub struct CorrelateArgs {
    /// telegram CSV file
    #[clap(short, long)]
    pub telegrams: Vec<String>,
    /// GPX-formatted gps track
    #[clap(short, long, required = true)]
    pub gps: Vec<String>,
    /// Legacy format gps data, you most probably don't need that
    #[clap(long)]
    pub gps_legacy: Vec<String>,
    /// Region number, see https://click.dvb.solutions/
    #[clap(short, long)]
    pub region: i32,
    /// wartrammer-40k json file with measured public transport runs
    #[clap(short, long)]
    pub wartrammer: Option<Vec<String>>,
    /// JSON outut file in stop-names format, if not specified result is printed on stdout
    #[clap(short, long)]
    pub stops_json: String,
    /// Geojson output for diagnostics
    #[clap(short = 'j', long)]
    pub geojson: Option<String>,
    /// Maximum time difference in seconds between gps point and telegram transmission time. Bigger
    /// values result in more transmission position matched at the cost of accuracy.
    #[clap(long, default_value = "5")]
    pub corr_window: i64,
    /// Telegram frequency in the region (in Hz), see https://click.dvb.solutions/
    #[clap(long)]
    pub meta_frequency: Option<u64>,
    /// Region name string, see https://click.dvb.solutions/
    #[clap(long)]
    pub meta_city: Option<String>,
}

#[derive(Args, Debug)]
pub struct MergeArgs {
    /// output directory in which stops.json-formatted files will be written
    #[clap(short, long, required = true)]
    pub out_dir: String,
    /// Input stops.json-formatted files
    #[clap(required = true)]
    pub stops: Vec<String>,
}

#[derive(Args, Debug)]
pub struct StopsToGeoArgs {
    /// Input stops.json files
    #[clap(required = true)]
    pub stops: Vec<String>,
    /// geojson file to write, if not specified geojson is dumped to stdout
    #[clap(short = 'o', long = "geojson")]
    pub geojson: Option<String>,
}

#[derive(Args, Debug)]
pub struct FilterArgs {
    /// csv with R09 telegrams
    #[clap(short, long, required = true)]
    pub telegrams: Vec<String>,
    /// wartrammer json output
    #[clap(short, long, required = true)]
    pub wartrammer: Vec<String>,
    /// output csv file to write.
    #[clap(short, long, required = true)]
    pub outfile: String,
}
