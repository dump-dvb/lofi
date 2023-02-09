use crate::gps::{Gps, GpsPoint};

use tlms::locations::{
    LocationsJson, RegionMetaInformation, RegionReportLocations, ReportLocation, REGION_META_MAP,
};
use tlms::telegrams::r09::R09SaveTelegram;

use std::collections::{HashMap, HashSet};

use log::{info, trace, warn};

/// Struct containing the transmission postion with private fields which are used to infer the
/// location of this telegram
#[derive(Debug)]
pub struct CorrTelegram {
    /// Transmission postion (meldepunkt) of the telegram
    pub transmission_position: i32,
    /// Unix timestamp of telegram interception time
    timestamp: i64,
    /// [`crate::gps::GpsPoint`] of a point that preceded directly before the telegram transmission (within
    /// correlation range_
    location_before: GpsPoint,
    /// [`crate::gps::GpsPoint`] of a point that followed directly after the telegram transmission (within
    /// correlation range_
    location_after: GpsPoint,
    /// region integer indentifier
    region: i64,
}

impl CorrTelegram {
    /// creates [`CorrTelegram`][crate::correlate::CorrTelegram] from [R09SaveTelegram][tlms::telegrams::r09::R09SaveTelegram] and two nearest [`GpsPoint`][crate::gps::GpsPoint]s
    pub fn new(tg: R09SaveTelegram, before: GpsPoint, after: GpsPoint) -> CorrTelegram {
        CorrTelegram {
            transmission_position: tg.reporting_point,
            timestamp: tg.time.timestamp(),
            location_before: before,
            location_after: after,
            region: tg.region,
        }
    }

    /// Converts [`CorrTelegram`] into a tuple of region identifier, meldepunkt and linearly
    /// interpolated location of the meldepunkt
    pub fn interpolate_position(&self) -> (i64, i32, ReportLocation) {
        (
            self.region,
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

/// function that performs full analysis of telegrams and gps positions, produces valid (and
/// hopefully production ready) [`LocationsJson`][tlms::locations::LocationsJson].
pub fn correlate(
    telegrams: Box<dyn Iterator<Item = R09SaveTelegram>>,
    gps: Gps,
    corr_window: i64,
) -> LocationsJson {
    // correlate telegrams to gps and for every telegram
    let ctg: Vec<CorrTelegram> = telegrams
        .filter_map(|t| correlate_telegram(&t, &gps, corr_window))
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

        // save the region list while we at it, so we can quickly add the [RegionMetaInformation]
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

    LocationsJson::construct(
        region_data,
        region_meta,
        Some(String::from(env!("CARGO_PKG_NAME"))),
        Some(String::from(env!("CARGO_PKG_VERSION"))),
    )
}

/// Creates  [`Option`]`<`[`crate::correlate::CorrTelegram`]`>` from [`tlms::telegrams::r09::R09SaveTelegram`]
/// and [`crate::gps::Gps`] taking the correlation window into account. Returns [`None`] if there's no
/// complete set of locations within correlation window (one before the telegram, one after).
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
