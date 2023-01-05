use serde::Deserialize;
use std::collections::HashMap;

use overpass_turbo::model::{Element, OverpassTurbo};
use overpass_turbo::simplified::{SimplifiedElement, SimplifiedOverpassTurbo};

#[derive(Deserialize)]
pub struct Ref {
    pub r#ref: String,
}

pub type OverpassIntermediate = HashMap<i32, Vec<Vec<(f64, f64)>>>;

pub fn extract_from_overpass(file: &str) -> OverpassIntermediate {
    let mut y = OverpassTurbo::from_file(file).unwrap();

    for (_key, value) in y.iter_mut() {
        match value {
            Element::Relation(rel) => {
                let mut new_members = Vec::new();
                for member in &rel.members {
                    if member.role != "stop" && member.role != "platform" {
                        new_members.push(member.clone());
                    }
                }
                rel.members = new_members.to_vec();
            }
            _ => {}
        }
    }

    let mut x = SimplifiedOverpassTurbo::from_struct(y);

    x.prune_nodes();
    x.prune_ways();

    // HashMap<String, Vec<Vec<(f32, f32)>>
    let mut coords_by_line: HashMap<i32, Vec<Vec<(f64, f64)>>> = HashMap::new();

    for (_key, value) in x.iter() {
        match value {
            SimplifiedElement::Relation(relation) => {
                let line;
                match serde_json::from_value::<Ref>(relation.tags.as_ref().unwrap().clone()) {
                    Ok(tags) => match tags.r#ref.parse::<i32>() {
                        Ok(number) => {
                            line = number;
                        }
                        Err(_) => {
                            continue;
                        }
                    },
                    Err(_) => {
                        continue;
                    }
                }

                let mut positions = Vec::new();

                for member in &relation.members {
                    positions.push((member.lat, member.lon));
                }

                match coords_by_line.get_mut(&line) {
                    Some(result) => {
                        result.push(positions);
                    }
                    None => {
                        coords_by_line.insert(line, vec![positions]);
                    }
                }
            }
            _ => {}
        }
    }

    coords_by_line
}
