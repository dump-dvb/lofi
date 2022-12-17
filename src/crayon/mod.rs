mod run_analyser;
mod overpass_extractor;

use crate::read_telegrams;
use crate::structs::CrayonArgs;

use chrono::Datelike;
use tlms::locations::{LocationsJson, RequestStatus};
use std::collections::HashMap;
use std::fs;

/// main function for crayon 
pub(crate) fn correlate_lines(args: CrayonArgs) {

    let mut current_day = 0;
    let mut daily_telegrams = Vec::new();
    let mut graph = HashMap::new();
    let mut graph_time = HashMap::new();
    let mut graph_lines = HashMap::new();

    // iterate over all the telegrams
    for telegram in read_telegrams(args.telegrams) {
                // check if the telegram is from the correct region
                if telegram.region == args.region {
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

                    if RequestStatus::from_i16(telegram.request_status).unwrap()
                        != RequestStatus::DoorClosed
                    {
                        daily_telegrams.push(telegram);
                    }
                }
    }

    let overpass_structs = overpass_extractor::extract_from_overpass(&args.overpass_turbo);
    let region_data = match LocationsJson::from_file(&args.stops_json) {
        Ok(locations) => {
            match locations.data.get(&args.region) {
                Some(data) => data.clone(),
                None => { 
                    println!("cannot read stops json");
                    return; 
                }
            }
        },
        Err(e) => {
            println!("error while trying to read overpass turbo file: {:?}", e);
            return
        }
    };

    let finished_graph = run_analyser::finalise(&graph, &graph_time);
    match args.geojson_graph {
        Some(file_path) => {
            println!("generating geojson for graph");
            run_analyser::geojson_draw_graph(&finished_graph, &region_data, &file_path);
        }
        None => {
            println!("no graph produced!");
        }
    }

    let raw_point_graph = run_analyser::generate_positions(&graph_lines, &graph_time, &finished_graph, &region_data, &overpass_structs);

    match args.geojson_points {
        Some(file_path) => {
            run_analyser::geojson_draw_points(&raw_point_graph, &file_path);
        }
        None => {

        }
    }

    let mut result = HashMap::new();
    result.insert(args.region, raw_point_graph);

    // writing graph_output
    fs::write(
        args.export,
        serde_json::to_string(&result).unwrap(),
    ).ok();

}
