use crate::types::R09Iter;

use std::fs::File;
use tlms::measurements::FinishedMeasurementInterval;

/// Takes an [`R09Iter`] and returns the iterator over telegrams matching the conditions in
/// supplied vector of [`FinishedMeasurementInterval`]
pub fn filter(unfiltered: R09Iter, wtfiles: Vec<String>) -> R09Iter {
    let mut wt: Vec<FinishedMeasurementInterval> = vec![];

    for wtfile in wtfiles {
        let rdr = File::open(wtfile).expect("Couldn't open wartrammer json");
        let mut wt_file: Vec<FinishedMeasurementInterval> =
            serde_json::from_reader(rdr).expect("Couldn't deserialize wartrammer json");
        wt.append(&mut wt_file);
    }

    eprintln!("got wt: {wt:#?}");

    Box::new(unfiltered.filter(move |t| wt.iter().any(|f| f.fits(t))))
}
