use crate::gps::{Gps, GpsPoint};
use crate::structs::{CorrTelegram, CorrelateArgs};
use crate::{filter, get_features, read_telegrams};

use dump_dvb::locations::{
    LocationsJson, RegionMetaInformation, RegionReportLocations, ReportLocation,
    REGION_META_MAP,
};
use dump_dvb::telegrams::r09::R09SaveTelegram;

use std::collections::HashMap;
use std::fs::write;

use geojson::FeatureCollection;
use log::{info, warn};

// Handles `lofi correlate`
pub fn correlate_cmd(cli: CorrelateArgs) {
    eprintln!("got args: {:?}", cli);
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

    eprintln!("Matched {} telegrams", ctg.len());

    // for every corrtelegram, interpolate the position from gps track
    let positions: Vec<(i32, ReportLocation)> =
        ctg.iter().map(|x| x.interpolate_position()).collect();

    // dedups locations
    let mut deduped_positions: HashMap<i32, ReportLocation> = HashMap::new();
    for (mp, pos) in positions {
        deduped_positions
            .entry(mp)
            .and_modify(|e| e.lat = (pos.lat + e.lat) / 2_f64)
            .and_modify(|e| e.lon = (pos.lon + e.lon) / 2_f64)
            .or_insert(pos);
    }

    fn project_epsg3857(loc: &ReportLocation) -> ReportLocation {
        const EARTH_RADIUS_M: f64 = 6_378_137_f64;
        let x = EARTH_RADIUS_M * loc.lon.to_radians();
        let y =
            ((loc.lat.to_radians() / 2. + std::f64::consts::PI / 4.).tan()).ln() * EARTH_RADIUS_M;

        ReportLocation {
            lat: loc.lat,
            lon: loc.lon,
            properties: match serde_json::from_str(&format!(
                "{{ \"epsg3857\": {{ \"x\":{}, \"y\":{} }} }}",
                x, y
            )) {
                Ok(val) => val,
                Err(whoopsie) => {
                    eprintln!("convert to pseudo-mercator: {}", whoopsie);
                    serde_json::Value::Null
                }
            },
        }
    }

    let deduped_positions: HashMap<i32, ReportLocation> = deduped_positions
        .into_iter()
        .map(|(k, v)| (k, project_epsg3857(&v)))
        .collect();

    // Constructing the stops.json
    let mut reg: RegionReportLocations = HashMap::new();
    for (mp, pos) in deduped_positions {
        reg.entry(mp).or_insert(pos);
    }

    let region_meta = match REGION_META_MAP.get(&cli.region) {
        Some(regio) => {
            info!("region no. {:?} lookup succesful: {:?}", reg, regio);
            regio.clone()
        }
        None => {
            warn!("Region {:?} is unknown! Is dump-dvb.rs updated?", cli.region);
            warn!("Lookup failed, populated region meta information from cli!");
            RegionMetaInformation {
                frequency: cli.meta_frequency,
                city_name: cli.meta_city,
                type_r09: None,
                lat: None,
                lon: None,
            }
        }
    };

    let stops = LocationsJson::construct(
        HashMap::from([(cli.region, reg)]),
        HashMap::from([(cli.region, region_meta)]),
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
