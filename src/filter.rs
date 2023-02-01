use std::fs::File;
use tlms::measurements::FinishedMeasurementInterval;
use tlms::telegrams::r09::R09SaveTelegram;

use crate::read_telegrams;
use crate::structs::FilterArgs;

// handles `lofi filter`
pub fn filter_cmd(opts: FilterArgs) {
    let tg = read_telegrams(opts.telegrams);
    let filtered = filter(tg, opts.wartrammer);
    let outfile = File::create(opts.outfile).expect("Couldn't create output file");
    let mut writer = csv::Writer::from_writer(outfile);
    filtered
        .filter_map(|t| writer.serialize(t).ok())
        .for_each(drop);
}

pub fn filter(
    unfiltered: Box<dyn Iterator<Item = R09SaveTelegram>>,
    wtfiles: Vec<String>,
) -> Box<dyn Iterator<Item = R09SaveTelegram>> {
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
