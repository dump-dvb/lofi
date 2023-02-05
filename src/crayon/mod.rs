pub mod overpass_extractor;
pub mod run_analyser;

use chrono::Datelike;
use std::collections::HashMap;
use std::fs;
use tlms::locations::{LocationsJson, RequestStatus, ReportLocation};
use tlms::telegrams::r09::R09SaveTelegram;

/// main function for crayon
pub(crate) fn correlate_lines(
    telegrams: Box<dyn Iterator<Item = R09SaveTelegram>>,
    region: i64,
    overpass_structs: HashMap<i32, Vec<Vec<(f64, f64)>>>,
    region_data: HashMap<i32, ReportLocation>,
    geojson_graph: Option<&str>,
    geojson_points: Option<&str>
) {
    let mut current_day = 0;
    let mut daily_telegrams = Vec::new();
    let mut graph = HashMap::new();
    let mut graph_time = HashMap::new();
    let mut graph_lines = HashMap::new();

    // iterate over all the telegrams
    for telegram in telegrams {
        // check if the telegram is from the correct region
        if telegram.region == region {
            if telegram.time.date().day() != current_day {
                println!("analysing day {:?}", &current_day);
                // calculate shit
                run_analyser::analyse_day(
                    &daily_telegrams,
                    &mut graph,
                    &mut graph_time,
                    &mut graph_lines,
                );
                //println!("new day: {} {}", telegram.time.date().month(), telegram.time.date().day());
                daily_telegrams.clear();
                current_day = telegram.time.date().day();
            }

            if RequestStatus::try_from(telegram.request_status).unwrap()
                != RequestStatus::DoorClosed
            {
                daily_telegrams.push(telegram);
            }
        }
    }

    let finished_graph = run_analyser::finalise(&graph, &graph_time);

    match geojson_graph {
        Some(file_path) => {
            println!("generating geojson for graph");
            run_analyser::geojson_draw_graph(&finished_graph, &region_data, &file_path);
        }
        None => {
            println!("no graph produced!");
        }
    }

    let raw_point_graph = run_analyser::generate_positions(
        &graph_lines,
        &graph_time,
        &finished_graph,
        &region_data,
        &overpass_structs,
    );

    match args.geojson_points {
        Some(file_path) => {
            run_analyser::geojson_draw_points(&raw_point_graph, &file_path);
        }
        None => {}
    }

    let mut result = HashMap::new();
    result.insert(region, raw_point_graph);

    // writing graph_output
    fs::write(args.export, serde_json::to_string(&result).unwrap()).ok();
}
