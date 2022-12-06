use chrono::Duration;

use dump_dvb::locations::graph::{LineSegment, RegionGraph, Position};
use dump_dvb::locations::RegionReportLocations;
use dump_dvb::telegrams::r09::R09SaveTelegram;
use geo_types::{coord, Geometry, GeometryCollection, Line, Point};
use geojson::FeatureCollection;
use geojson::Feature; 
use random_color::RandomColor;
use std::hash::Hash;
use std::{
    collections::{HashMap, HashSet},
    fs,
};

pub type Coordinate = (f64, f64);

// (reporting_point, direction) -> reporting_point -> (amount, time difference, line)
pub type GraphRater = HashMap<(i32, i32), i32>;
pub type GraphRaterTime = HashMap<(i32, i32), Vec<i64>>;
pub type GraphLines = HashMap<(i32, i32), HashSet<i32>>;

// (reporting_point, direction) -> reporting_point
pub type Graph = HashMap<i32, Vec<i32>>;

pub type OverPassTurboExport = HashMap<i32, Vec<Vec<(f64, f64)>>>;

const TIME_THRESHOLD: i64 = 10;

pub fn find_closest_on_track(segment: &Vec<(f64, f64)>, coordinate: &(f64, f64)) -> (usize, f64){
    let distance = |x: &(f64, f64), y: &(f64, f64)| -> f64 {
        f64::sqrt((x.0 - y.0).powf(2f64) + (x.1 - y.1).powf(2f64))
    };
    let mut index = 0;
    let mut best_distance = f64::MAX;

    for i in 0..segment.len() {
        let current_distance = distance(coordinate, &segment[i]);

        if current_distance < best_distance {
            index = i;
            best_distance = current_distance;
        }
    }

    (index, best_distance)
}


// This function goes over all the lines exported from overpass-turbo 
pub fn find_ideal_track(
    lines: &HashSet<i32>,
    line_coordinates: &OverPassTurboExport,
    previous_coords: &Coordinate,
    next_coords: &Coordinate,
) -> Vec<Coordinate> {

    let mut best_line_index = 0;
    let mut best_line_config = (0, 0, 0);
    let mut best_line_distance = f64::MAX;

    for (_line_number, line) in lines.iter().enumerate() {
        line_coordinates.get(line).map(|coord_lines| {
            let mut best_segment_index = 0;
            let mut best_segment_distance = f64::MAX;
            let mut best_segment_interval = (0, 0);

            for (segment_number, segment) in coord_lines.iter().enumerate() {
                let (start_index, start_distance) = find_closest_on_track(segment, previous_coords);
                let (end_index, end_distance) = find_closest_on_track(segment, next_coords);

                if start_index < end_index && (start_distance + end_distance) < best_segment_distance {
                    best_segment_distance = start_distance + end_distance;
                    best_segment_index = segment_number;
                    best_segment_interval = (start_index, end_index);

                }
            }

            if best_segment_distance < best_line_distance{
                println!("{}", &best_segment_distance);
                best_line_distance = best_segment_distance;
                best_line_index = *line;
                best_line_config = (
                    best_segment_index,
                    best_segment_interval.0,
                    best_segment_interval.1,
                );
            }
        });
    }

    if best_line_distance < f64::MAX {
        let mut vec: Vec<Coordinate> = Vec::new();
        line_coordinates.get(&(best_line_index as i32)).map(|lines| {
            for (_i, point) in lines[best_line_config.0][best_line_config.1..best_line_config.2]
                .iter()
                .enumerate()
            {
                vec.push(*point);
            }
            [best_line_config.0]
        });

        println!(
            "ideal config {} {:?} distance: {} points: {}",
            &best_line_index,
            &best_line_config,
            &best_line_distance,
            vec.len()
        );
        vec
    } else {
        return Vec::new()
    }
}

pub fn convert_list (vec_data: Vec<(f64, f64)>) -> HashMap<String, Position> {
    let mut transposed_coords: HashMap<String, Position> = HashMap::new();

    for (i, coords) in vec_data.iter().enumerate() {
        let position = Position {
            lat: coords.0 as f32,
            lon: coords.1 as f32,
            properties: HashMap::new()
        };

        transposed_coords.insert((((i as f32) / (vec_data.len() as f32) * 100f32) as i32).to_string(), position);
    }

    return transposed_coords;
}

pub fn generate_positions(
    line_graph: &GraphLines,
    graph_time: &GraphRaterTime,
    graph: &Graph,
    region: &RegionReportLocations,
    overpass_turbo: &HashMap<i32, Vec<Vec<(f64, f64)>>>,
) -> RegionGraph {
    let mut export: RegionGraph = HashMap::new();

    for (previous, list_nexts) in graph {
        // start coordinate
        let previous_coords = match region.get(previous) {
            Some(coords) => (coords.lat as f64, coords.lon as f64),
            None => {
                continue;
            }
        };

        for next in list_nexts {
            // end coordinates
            let next_coords = match region.get(next) {
                Some(coords) => (coords.lat as f64, coords.lon as f64),
                None => {
                    continue;
                }
            };

            match line_graph.get(&(*previous, *next)) {
                Some(lines) => match export.get_mut(previous) {
                    Some(value) => {
                        let mean_time = match graph_time.get(&(*previous, *next)) {
                            Some(times) => {
                                ((times.iter().sum::<i64>() as f64 / times.len() as f64)) as u32
                            }
                            None => 120,
                        };

                        // here we are getting a list of coordinates 
                        let mut vec_data = find_ideal_track(
                            lines,
                            &overpass_turbo,
                            &previous_coords,
                            &next_coords,
                        );
                        vec_data.insert(0, previous_coords);
                        vec_data.push(next_coords);


                        let line_segment = LineSegment {
                            historical_time: mean_time,
                            next_reporting_point: *next,
                            positions: convert_list(vec_data),
                        };

                        value.push(line_segment);
                    }
                    None => {
                        let mean_time = match graph_time.get(&(*previous, *next)) {
                            Some(times) => {
                                ((times.iter().sum::<i64>() as f64 / times.len() as f64)) as u32
                            }
                            None => 120,
                        };

                        let mut vec_data = find_ideal_track(
                            lines,
                            &overpass_turbo,
                            &previous_coords,
                            &next_coords,
                        );
                        vec_data.insert(0, previous_coords);
                        vec_data.push(next_coords);

                        let line_segment = LineSegment {
                            historical_time: mean_time,
                            next_reporting_point: *next,
                            positions: convert_list(vec_data),
                        };
                        export.insert(*previous, vec![line_segment]);
                    }
                },
                None => {
                    continue;
                }
            }
        }
    }
    println!("{:?}", &export);

    export
}

pub fn analyse_day(
    telegrams: &Vec<R09SaveTelegram>,
    graph: &mut GraphRater,
    graph_time: &mut GraphRaterTime,
    graph_lines: &mut GraphLines,
) {
    for (i, telegram) in telegrams.iter().enumerate() {
        let mut next_occurance = None;
        for j in i + 1..telegrams.len() {
            if telegrams[j].line == telegram.line && telegrams[j].run_number == telegram.run_number
            {
                next_occurance = Some(telegrams[j].clone());
                break;
            }

            if (telegrams[j].time - telegram.time) > Duration::minutes(TIME_THRESHOLD) {
                break;
            }
        }

        match next_occurance {
            Some(next_telegram) => {
                let key = (telegram.reporting_point, next_telegram.reporting_point);
                if graph.contains_key(&key) {
                    let current = graph.get_mut(&key).unwrap();
                    *current += 1;
                    graph_time
                        .get_mut(&key)
                        .unwrap()
                        .push((next_telegram.time - telegram.time).num_milliseconds());
                    graph_lines
                        .get_mut(&key)
                        .unwrap()
                        .insert(telegram.line.unwrap());
                } else {
                    graph.insert(key, 1);
                    graph_time.insert(
                        key,
                        vec![(next_telegram.time - telegram.time).num_milliseconds()],
                    );
                    graph_lines.insert(key, HashSet::from([telegram.line.unwrap()]));
                }
            }
            _ => {}
        }
    }
}

pub fn rate(count: i32, times: &Vec<i64>) -> f64 {
    let mean = times.iter().sum::<i64>() as f64 / times.len() as f64;

    const EXPECTED_AVERAGE_TRAVEL_TIME: f64 = 120f64;

    let rating = 
        50.0f64 * std::f64::consts::E.powf(-1f64 * f64::abs(mean - EXPECTED_AVERAGE_TRAVEL_TIME)) 
        + 0.01f64 * (count as f64);

    println!("rating: {}, mean: {}, count: {}", &rating, &rating, count);

    rating
}

pub fn finalise(rated_graph: &GraphRater, time_graph: &GraphRaterTime) -> Graph {
    const RATING_THRESHHOLD: f64 = 2f64;
    const TIME_THRESHHOLD: u64 = 300;

    let mut graph = HashMap::<i32, Vec<i32>>::new();
    let mut average_list = Vec::new();

    for (key, value) in &*rated_graph {
        average_list.push(rate(*value, time_graph.get(key).unwrap()));
    }

    let mu = average_list.iter().sum::<f64>() as f64 / average_list.len() as f64;
    let sigma = f64::sqrt(
        average_list
            .iter()
            .map(|x| (x - mu) * (x - mu))
            .sum::<f64>()
            / average_list.len() as f64,
    );

    for (key, value) in &*rated_graph {
        let rating = rate(*value, time_graph.get(key).unwrap());
        if rating > RATING_THRESHHOLD * sigma {
            if graph.contains_key(&key.0) {
                graph.get_mut(&key.0).map(|x| x.push(key.1));
            } else {
                graph.insert(key.0, vec![key.1]);
            }
        }
    }
    graph
}

pub fn geojson_draw_graph(graph: &Graph, region: &RegionReportLocations, export_file: &str) {
    let mut ignore_ids = Vec::new();
    let mut geojson_data = Vec::new();

    for (key, value) in &*graph {
        let point_1_coords;
        match region.get(key) {
            Some(coords) => {
                point_1_coords = coords;

                if !ignore_ids.contains(key) {
                    geojson_data.push(Geometry::from(Point(
                        coord! { x: coords.lon, y: coords.lat },
                    )));
                    ignore_ids.push(*key);
                }
            }
            None => {
                continue;
            }
        }

        for x in value {
            match region.get(x) {
                Some(coords) => {
                    if !ignore_ids.contains(x) {
                        geojson_data.push(Geometry::from(Point(
                            coord! { x: coords.lon, y: coords.lat},
                        )));
                        ignore_ids.push(*key);
                    }
                    geojson_data.push(Geometry::from(Line {
                        end: coord! {x: point_1_coords.lon, y: point_1_coords.lat},
                        start: coord! {x: coords.lon, y: coords.lat},
                    }));
                }
                None => {}
            }
        }
    }

    let geometry_collection = GeometryCollection::from_iter(geojson_data);
    let feature_collection = FeatureCollection::from(&geometry_collection);

    fs::write(
        export_file,
        serde_json::to_string(&feature_collection).unwrap(),
    ).ok();
}

pub fn geojson_draw_points(export: &RegionGraph, export_file: &str) {
    let mut geojson_data = Vec::new();
    for (_source, segments) in export {
        for segment in segments {
            let mut properties = geojson::JsonObject::new();
            let key = "color".to_string();
            properties.insert(key, geojson::JsonValue::from(RandomColor::new().to_hex()));

            for (key, point) in &segment.positions {
                let geometry = geojson::Geometry::new(geojson::Value::Point(vec![
                    point.lat as f64,
                    point.lon as f64,
                ]));
                let feature = Feature {
                    bbox: None,
                    geometry: Some(geometry),
                    id: None,
                    properties: Some(properties.clone()),
                    foreign_members: None,
                };

                geojson_data.push(feature);
            }
        }
    }

    //let geometry_collection = GeometryCollection::from_iter(geojson_data);
    let feature_collection = FeatureCollection {
        bbox: None,
        features: geojson_data,
        foreign_members: None,
    };

    fs::write(
        export_file,
        serde_json::to_string(&feature_collection).unwrap(),
    ).ok();
}
