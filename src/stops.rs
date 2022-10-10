use crate::gps::{Gps, GpsPoint};
use crate::structs::{CorrTelegram, CorrelateArgs};

use dump_dvb::locations::{
    LocationsJson, R09Types, RegionMetaInformation, RegionReportLocations, ReportLocation,
};
use dump_dvb::telegrams::r09::R09SaveTelegram;

use std::collections::HashMap;
use std::fs::write;

use geojson::FeatureCollection;

use crate::{filter, read_telegrams, get_features};

pub fn correlate(cli: CorrelateArgs) {
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
pub fn correlate_telegram(
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

