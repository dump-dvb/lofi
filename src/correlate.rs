use tlms::locations::gps::GpsPoint;
use tlms::locations::{ApiTransmissionLocation, InsertTransmissionLocationRaw};
use tlms::telegrams::r09::R09SaveTelegram;

use uuid::Uuid;

use std::collections::HashMap;

/// default correlation window in seconds
pub const DEFAULT_CORRELATION_WINDOW: i64 = 7;

/// Struct containing the transmission postion with private fields which are used to infer the
/// location of this telegram
#[derive(Debug)]
pub struct CorrTelegram {
    /// Transmission postion (meldepunkt) of the telegram
    pub reporting_point: i32,
    /// Unix timestamp of telegram interception time
    timestamp: i64,
    /// [`GpsPoint`] of a point that preceded directly before the telegram transmission (within
    /// correlation range_
    location_before: GpsPoint,
    /// [`GpsPoint`] of a point that followed directly after the telegram transmission (within
    /// correlation range_
    location_after: GpsPoint,
    /// region integer indentifier
    region: i64,
    /// from which trekkie run it was generated
    trekkie_run: Option<Uuid>,
    /// who owned a trekkie run
    run_owner: Option<Uuid>,
}

impl TryFrom<CorrTelegram> for InsertTransmissionLocationRaw {
    type Error = &'static str;
    fn try_from(value: CorrTelegram) -> Result<Self, Self::Error> {
        let trekkie_run = match value.trekkie_run {
            Some(r) => r,
            None => {
                return Err("trekkie run is not set!");
            }
        };
        let run_owner = match value.run_owner {
            Some(r) => r,
            None => {
                return Err("trekkie run is not set!");
            }
        };
        Ok(InsertTransmissionLocationRaw {
            id: None,
            region: value.region,
            reporting_point: value.reporting_point,
            lat: value.location_before.lat
                + (value.timestamp - value.location_before.timestamp.timestamp()) as f64
                    / (value.location_after.timestamp.timestamp()
                        + value.location_before.timestamp.timestamp()) as f64
                    * (value.location_after.lat - value.location_before.lat),
            lon: value.location_before.lon
                + (value.timestamp - value.location_before.timestamp.timestamp()) as f64
                    / (value.location_after.timestamp.timestamp()
                        + value.location_before.timestamp.timestamp()) as f64
                    * (value.location_after.lon - value.location_before.lon),
            trekkie_run,
            run_owner,
        })
    }
}

impl CorrTelegram {
    /// creates [`CorrTelegram`][crate::correlate::CorrTelegram] from [R09SaveTelegram][tlms::telegrams::r09::R09SaveTelegram] and two nearest [`GpsPoint`]'s
    pub fn new(
        tg: R09SaveTelegram,
        before: GpsPoint,
        after: GpsPoint,
        trekkie_run: Uuid,
        run_owner: Uuid,
    ) -> CorrTelegram {
        CorrTelegram {
            reporting_point: tg.reporting_point,
            timestamp: tg.time.timestamp(),
            location_before: before,
            location_after: after,
            region: tg.region,
            trekkie_run: Some(trekkie_run),
            run_owner: Some(run_owner),
        }
    }

    /// Converts [`CorrTelegram`] into a tuple of region identifier, meldepunkt and linearly
    /// interpolated location of the meldepunkt
    pub fn interpolate_position(&self) -> (i64, i32, ApiTransmissionLocation) {
        (
            self.region,
            self.reporting_point,
            ApiTransmissionLocation {
                lat: self.location_before.lat
                    + (self.timestamp - self.location_before.timestamp.timestamp()) as f64
                        / (self.location_after.timestamp.timestamp()
                            + self.location_before.timestamp.timestamp())
                            as f64
                        * (self.location_after.lat - self.location_before.lat),
                lon: self.location_before.lon
                    + (self.timestamp - self.location_before.timestamp.timestamp()) as f64
                        / (self.location_after.timestamp.timestamp()
                            + self.location_before.timestamp.timestamp())
                            as f64
                        * (self.location_after.lon - self.location_before.lon),
                properties: serde_json::Value::Null,
            },
        )
    }
}

/// Error type for correlate function
pub enum CorrelateError {
    /// No appropriate input recieved
    EmptyInput,
    /// [Some of] the data is for the inappropriate region
    RegionMismatch,
}

/// Function correlates telegrams to locations within one trekkie run, and designed to be used
/// within [trekkie][<https://github.com/tlm-solutions/trekkie>]. Returned vector is ready to
/// insert into the appropriate DB table.
pub fn correlate_trekkie_run(
    telegrams: &Vec<R09SaveTelegram>,
    gps: Vec<GpsPoint>,
    corr_window: i64,
    trekkie_run: Uuid,
    run_owner: Uuid,
) -> Result<Vec<InsertTransmissionLocationRaw>, CorrelateError> {
    if telegrams.is_empty() {
        return Err(CorrelateError::EmptyInput);
    }

    let gps = gps
        .into_iter()
        .map(|v| (v.timestamp.timestamp(), v))
        .collect::<HashMap<i64, GpsPoint>>();

    let correlated_telegrams: Vec<CorrTelegram> = telegrams
        .iter()
        .filter_map(|t| {
            correlate_trekkie_run_telegram(t, &gps, corr_window, trekkie_run, run_owner)
        })
        .collect();

    // for every corrtelegram, interpolate the position from gps track
    Ok(correlated_telegrams
        .into_iter()
        .filter_map(|x| x.try_into().ok())
        .collect::<Vec<InsertTransmissionLocationRaw>>())
}

/// Creates  [`crate::correlate::CorrTelegram`] from [`tlms::telegrams::r09::R09SaveTelegram`]
/// and [`Gps`] taking the correlation window into account. Returns [`None`] if there's no
/// complete set of locations within correlation window (one before the telegram, one after).
pub fn correlate_trekkie_run_telegram(
    telegram: &R09SaveTelegram,
    gps: &HashMap<i64, GpsPoint>,
    corr_window: i64,
    trekkie_run: Uuid,
    run_owner: Uuid,
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
        (Some(before_point), Some(after_point)) => Some(CorrTelegram::new(
            telegram.clone(),
            **before_point,
            **after_point,
            trekkie_run,
            run_owner,
        )),
        _ => None,
    }
}
