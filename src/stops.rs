use crate::gps::{Gps, GpsPoint};
use crate::structs::{CorrTelegram, CorrelateArgs};
use crate::{filter, get_features, read_telegrams};

use tlms::locations::{
    LocationsJson, RegionMetaInformation, RegionReportLocations, ReportLocation, REGION_META_MAP,
};
use tlms::telegrams::r09::R09SaveTelegram;

use std::collections::{HashMap, HashSet};
use std::fs::write;

use geojson::FeatureCollection;
use log::{info, trace, warn};

// Handles `lofi correlate`
pub fn correlate_cmd(cli: CorrelateArgs) {
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

    // correlate telegrams to gps and for every telegram
    let ctg: Vec<CorrTelegram> = telegrams
        .filter_map(|t| correlate_telegram(&t, &gps, cli.corr_window))
        .collect();

    info!("Matched {} telegrams", ctg.len());

    // for every corrtelegram, interpolate the position from gps track
    let positions: Vec<(i64, i32, ReportLocation)> =
        ctg.iter().map(|x| x.interpolate_position()).collect();

    // dedups locations, take average between new telegram and already existing one, if
    // nonexistent, then insert
    let mut deduped_positions: HashMap<(i64, i32), ReportLocation> = HashMap::new();
    for (reg, mp, pos) in positions {
        deduped_positions
            .entry((reg, mp))
            .and_modify(|e| e.lat = (pos.lat + e.lat) / 2_f64)
            .and_modify(|e| e.lon = (pos.lon + e.lon) / 2_f64)
            .or_insert(pos);
    }

    // update locations with epsg3857 coordinates
    deduped_positions
        .values_mut()
        .into_iter()
        .for_each(|loc| loc.update_epsg3857());

    // Constructing the stops.json
    // colect all deduped loc positions
    let mut region_data: HashMap<i64, RegionReportLocations> = HashMap::new();
    let mut regions: HashSet<i64> = HashSet::new();
    for ((reg, mp), pos) in &deduped_positions {
        region_data
            .entry(*reg) // get the region value
            .or_insert(HashMap::from([(*mp, pos.clone())])) // If not exists, put hashmap as value
            .insert(*mp, pos.clone()); // or just add the mp, ReportLocation pair

        // save the region list while we at it, so we can quickly add the `RegionMetaInformation`
        // after
        if regions.insert(*reg) {
            trace!("Found region no. {} from parsing telegrams", reg)
        }
    }

    let region_meta: HashMap<i64, RegionMetaInformation> = regions
        .iter()
        .map(|reg| match REGION_META_MAP.get(reg) {
            Some(r) => (*reg, r.clone()),
            None => {
                warn!("Could not find region no. {}! Is tlms.rs updated?", reg);
                warn!(
                    "filling RegionMetaInformation with null values for region {}!",
                    reg
                );
                (
                    *reg,
                    RegionMetaInformation {
                        frequency: None,
                        city_name: None,
                        type_r09: None,
                        lat: None,
                        lon: None,
                    },
                )
            }
        })
        .collect();

    let stops = LocationsJson::construct(
        region_data,
        region_meta,
        Some(String::from(env!("CARGO_PKG_NAME"))),
        Some(String::from(env!("CARGO_PKG_VERSION"))),
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
        (Some(a), Some(b)) => Some(CorrTelegram::new(telegram.clone(), **b, **a)),
        _ => None,
    }
}
