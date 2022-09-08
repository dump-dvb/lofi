use std::collections::HashMap;
use std::fs::{write, File};

use chrono;
use clap::Parser;
use serde_json;

use dump_dvb::locations::{
    DocumentMetaInformation, InterRegional, R09Types, RegionMetaInformation,
    RegionalTransmissionPositions, TransmissionPosition,
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
            tg.into_iter().filter_map(|x| writer.serialize(x).ok()).for_each(drop);
        }
    }
}

fn merge(opts: MergeArgs) {
    let mut dedup: HashMap<i32, HashMap<(i32, i16, i16), TransmissionPosition>> = HashMap::new();
        let mut regions: Vec<i32> = vec![];
    for path in opts.stops {
        let stops = InterRegional::from(&path).unwrap();
        for (reg, regval) in stops.data {
            regions.push(reg);
            for (lsa, pos) in regval {
                for p in pos {
                    let val = dedup.entry(reg).or_insert(HashMap::new());
                    val.entry((lsa, p.direction, p.clone().request_status as i16))
                        .and_modify(|old| old.lat = (old.lat + p.lat) / 2.0)
                        .and_modify(|old| old.lon = (old.lon + p.lon) / 2.0)
                        .or_insert(p);
                }
            }
        }
    }


    let document_meta = DocumentMetaInformation {
        schema_version: String::from("2"),
        date: chrono::Utc::now(),
        generator: Some(String::from("lofi")),
        generator_version: Some(String::from(env!("CARGO_PKG_VERSION"))),
    };

    let region_dummy_meta = RegionMetaInformation {
            frequency: None,
            city_name: None,
            type_r09: None,
            lat: None,
            lon: None,
        };

    let mut all_regions: HashMap<i32, RegionalTransmissionPositions>= HashMap::new();
    let mut all_meta: HashMap<i32, RegionMetaInformation> = HashMap::new();

    for r in regions {
        let mut region_out: RegionalTransmissionPositions = HashMap::new();
        let dedup_reg = dedup.get(&r).unwrap().clone();
        for ((lsa, _dir, _req), pos) in dedup_reg {
            region_out.entry(lsa).or_insert(Vec::new()).push(pos);
        }

        all_regions.insert(r, region_out.clone());
        // TODO despaggetify so this can be properly set
        all_meta.insert(r, region_dummy_meta.clone());

        let region_json = InterRegional {
            document: document_meta.clone(),
            data: HashMap::from([(r, region_out)]),
            meta: HashMap::from([(r, region_dummy_meta.clone())]),
        };

        region_json.write(&(format!("{}/{}.json", opts.out_dir, r)));
    }

    let all_json = InterRegional {
        document: document_meta,
        data: all_regions,
        meta: all_meta,
    };
        all_json.write(&(format!("{}/all.json", opts.out_dir)));

}

fn stops2geo(opts: StopsToGeoArgs) {
    let mut features: Vec<Feature> = vec![];
    for path in opts.stops {
        let stops = InterRegional::from(&path).expect("Couldn't deserialize stops file");
        for (_k, v) in stops.data {
            features.append(&mut get_features(&v));
        }
    }

    let feature_collection = FeatureCollection {
        bbox: None,
        features: features,
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

    let mut telegrams = vec![];
    for w in wt {
        let mut tg: Vec<R09SaveTelegram> = unfiltered
            .iter()
            .filter_map(|a| if w.fits(&a) { Some(a) } else { None })
            .cloned()
            .collect();
        telegrams.append(&mut tg);
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
    let positions: Vec<(i32, TransmissionPosition)> =
        ctg.iter().map(|x| x.interpolate_position()).collect();

    // dedups locations
    let mut deduped_positions: HashMap<(i32, i16, i16), TransmissionPosition> = HashMap::new();
    for (lsa, pos) in positions {
        deduped_positions
            .entry((lsa, pos.direction, pos.request_status.clone() as i16))
            .and_modify(|e| e.lat = (pos.lat + e.lat) / 2 as f64)
            .and_modify(|e| e.lon = (pos.lon + e.lon) / 2 as f64)
            .or_insert(pos);
    }

    // Constructing the stops.json
    let mut reg: RegionalTransmissionPositions = HashMap::new();
    for ((lsa, _dir, _req), pos) in deduped_positions {
        reg.entry(lsa).or_insert(Vec::new()).push(pos);
    }

    let document_meta = DocumentMetaInformation {
        schema_version: String::from("2"),
        date: chrono::Utc::now(),
        generator: Some(String::from("lofi")),
        generator_version: Some(String::from(env!("CARGO_PKG_VERSION"))),
    };

    let region_meta = RegionMetaInformation {
        frequency: cli.meta_frequency,
        city_name: cli.meta_city,
        type_r09: Some(R09Types::R16),
        lat: None,
        lon: None,
    };

    let stops = InterRegional {
        document: document_meta,
        data: HashMap::from([(cli.region, reg.clone())]),
        meta: HashMap::from([(cli.region, region_meta)]),
    };

    stops.write(&cli.stops_json);

    if let Some(path) = cli.geojson {
        let features = get_features(&reg);
        let feature_collection = FeatureCollection {
            bbox: None,
            features: features,
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

fn get_features(data: &RegionalTransmissionPositions) -> Vec<Feature> {
    let mut features: Vec<Feature> = vec![];
    for (n, v) in data.iter() {
        //eprintln!("{:?}, {:?}", n, v);
        for x in v {
            let mut properties = JsonObject::new();
            let propval = format!("lsa:{} dir:{} type:{:?}", n, x.direction, x.request_status);
            properties.insert("name".to_string(), JsonValue::from(propval));
            features.push(Feature {
                bbox: None,
                geometry: Some(Geometry::new(Value::Point(vec![x.lon, x.lat]))),
                id: None,
                properties: Some(properties),
                foreign_members: None,
            })
        }
    }

    features
}
