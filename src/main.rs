use std::collections::HashMap;
use std::fs::{write, File};

use chrono;
use clap::Parser;
use serde_json;

use dump_dvb::locations::{
    self, LocationsJson, R09Types, RegionMetaInformation, RegionReportLocations, ReportLocation,
};
use dump_dvb::measurements::FinishedMeasurementInterval;
use dump_dvb::telegrams::r09::R09SaveTelegram;

mod gps;
mod structs;
use gps::{Gps, GpsPoint};
use structs::{Cli, Command, CorrTelegram, CorrelateArgs, MergeArgs, StopsToGeoArgs};

use geojson::{Feature, FeatureCollection, Geometry, JsonObject, JsonValue, Value};

fn main() {
    let cli = Cli::parse();
    eprintln!("{:#?}", cli);

    match cli.command {
        Command::Correlate(opts) => correlate(opts),
        Command::Merge(opts) => merge(opts),
        Command::StopsToGeo(opts) => stops2geo(opts),
        Command::Filter(opts) => {
            let tg = filter(read_telegrams(opts.telegrams), opts.wartrammer);
            let file = File::create(opts.outfile).expect("Couldn't create output file");
            let mut writer = csv::Writer::from_writer(file);
            tg.into_iter()
                .filter_map(|x| writer.serialize(x).ok())
                .for_each(drop);
        }
    }
}

fn merge(opts: MergeArgs) {
    // another good point - how do we want to structure the shit
    todo!("Not implemented yet for new format");
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
            println!("{}", geojson_string);
        }
    }
}

fn filter(unfiltered: Vec<R09SaveTelegram>, wtfiles: Vec<String>) -> Vec<R09SaveTelegram> {
    let mut wt: Vec<FinishedMeasurementInterval> = vec![];
    for wtfile in wtfiles {
        let rdr = File::open(wtfile).expect("Couldn't open wartrammer json");
        let mut wt_file: Vec<FinishedMeasurementInterval> =
            serde_json::from_reader(rdr).expect("Couldn't deserialize wartrammer json");
        wt.append(&mut wt_file);
    }
    eprintln!("using times.json: {:#?}", wt);

    //TODO
    let mut telegrams = vec![];
    for w in wt {
        let mut fit = 0;
        let mut didnt = 0;
        let mut tg: Vec<R09SaveTelegram> = unfiltered
            .iter()
            // Here we also need to check against region, but telegram dumps don't have it atm
            .filter_map(|a| if w.fits(&a) { Some(a) } else { None })
            .cloned()
            .filter_map(|a| {
                if w.fits(&a) {
                    fit += 1;
                    Some(a)
                } else {
                    didnt += 1;
                    None
                }
            })
            .collect();
        telegrams.append(&mut tg);
        eprintln!("processed: {}; fit: {}; didnt: {}", fit + didnt, fit, didnt);
    }
    telegrams
}

fn read_telegrams(paths: Vec<String>) -> Vec<R09SaveTelegram> {
    let mut telegrams_unfiltered: Vec<R09SaveTelegram> = vec![];
    for filepath in paths {
        let file = File::open(filepath).unwrap();
        let mut reader = csv::Reader::from_reader(file);
        for result in reader.deserialize::<R09SaveTelegram>() {
            match result {
                Ok(record) => {
                    telegrams_unfiltered.push(record);
                }
                Err(whoopsie) => {
                    eprintln!("Couldn't deserialize telegram! {}", whoopsie);
                    continue;
                }
            }
        }
    }
    telegrams_unfiltered
}

fn correlate(cli: CorrelateArgs) {
    let telegrams = match cli.wartrammer {
        Some(wtfiles) => filter(read_telegrams(cli.telegrams), wtfiles),
        None => read_telegrams(cli.telegrams),
    };

    let mut gps: Gps = Gps::empty();
    for filepath in cli.gps {
        gps.insert_from_gpx_file(&filepath);
    }
    for filepath in cli.gps_legacy {
        gps.insert_from_legacy(&filepath);
    }

    let ctg: Vec<CorrTelegram> = telegrams
        .iter()
        .filter_map(|tg| correlate_telegram(tg, &gps, cli.corr_window))
        .collect();

    // for every corrtelegram, interpolate the position from gps track
    let positions: Vec<(i32, ReportLocation)> =
        ctg.iter().map(|x| x.interpolate_position()).collect();

    // dedups locations
    let mut deduped_positions: HashMap<i32, ReportLocation> = HashMap::new();
    for (mp, pos) in positions {
        deduped_positions
            .entry(mp)
            .and_modify(|e| e.lat = (pos.lat + e.lat) / 2 as f64)
            .and_modify(|e| e.lon = (pos.lon + e.lon) / 2 as f64)
            .or_insert(pos);
    }

    // Constructing the stops.json
    let mut reg: RegionReportLocations = HashMap::new();
    for (mp, pos) in deduped_positions {
        reg.entry(mp).or_insert(pos);
    }

    let region_meta = RegionMetaInformation {
        frequency: cli.meta_frequency,
        city_name: cli.meta_city,
        type_r09: Some(R09Types::R16),
        lat: None,
        lon: None,
    };

    let stops = LocationsJson::construct(
        HashMap::from([(cli.region, reg)]),
        HashMap::from([(cli.region, region_meta)]),
        None,
        None,
    );

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

/// Correlates the telegrams
fn correlate_telegram(
    telegram: &R09SaveTelegram,
    gps: &Gps,
    corr_window: i64,
) -> Option<CorrTelegram> {
    let after: Vec<&GpsPoint> = (0..corr_window)
        .collect::<Vec<i64>>()
        .into_iter()
        .filter_map(|x| gps.get(&(telegram.time.timestamp() + x)))
        .collect();

    let before: Vec<&GpsPoint> = (-corr_window..0)
        .rev()
        .collect::<Vec<i64>>()
        .into_iter()
        .filter_map(|x| gps.get(&(telegram.time.timestamp() + x)))
        .collect();

    match (before.get(0), after.get(0)) {
        (Some(a), Some(b)) => CorrTelegram::new(telegram.clone(), **b, **a),
        _ => None,
    }
}

fn get_features(locs: &LocationsJson) -> Vec<Feature> {
    let mut features: Vec<Feature> = vec![];
    for (n, v) in locs.data.iter() {
        //eprintln!("{:?}, {:?}", n, v);
        for (mp, loc) in v {
            let mut properties = JsonObject::new();
            let propval = format!("{}", mp);
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
