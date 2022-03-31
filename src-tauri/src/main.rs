#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Mutex,
};

use ffxiv_crafting::{Attributes, CastActionError, Recipe, Skills, Status};
use serde::Serialize;

mod solver;

#[tauri::command(async)]
fn new_recipe(
    rlv: i32,
    difficulty_factor: u16,
    quality_factor: u16,
    durability_factor: u16,
) -> Recipe {
    Recipe::new(rlv, difficulty_factor, quality_factor, durability_factor)
}

#[tauri::command(async)]
fn new_status(attrs: Attributes, recipe: Recipe, init_quality: u32) -> Status {
    let mut s = Status::new(attrs, recipe);
    s.quality = init_quality;
    s
}

#[derive(Serialize)]
struct CastErrorPos {
    pos: usize,
    err: CastActionError,
}

#[derive(Serialize)]
struct SimulateResult {
    status: Status,
    errors: Vec<CastErrorPos>,
}

#[tauri::command(async)]
fn simulate(status: Status, skills: Vec<Skills>) -> SimulateResult {
    let mut result = SimulateResult {
        status,
        errors: Vec::new(),
    };
    for (pos, sk) in skills.iter().enumerate() {
        match result.status.is_action_allowed(*sk) {
            Ok(_) => result.status.cast_action(*sk),
            Err(err) => result.errors.push(CastErrorPos { pos, err }),
        }
    }
    result
}

#[derive(Serialize)]
struct RecipeRow {
    id: usize,
    rlv: i32,
    name: String,
    job: String,

    difficulty_factor: u16,
    quality_factor: u16,
    durability_factor: u16,
}

#[tauri::command(async)]
fn recipe_table() -> Vec<RecipeRow> {
    csv::ReaderBuilder::new()
        .comment(Some(b'#'))
        .from_reader(include_bytes!("../assets/Recipe.csv").as_slice())
        .records()
        .map(|row| {
            let row = row?;
            Ok(RecipeRow {
                id: row.get(1).unwrap().parse().unwrap(),
                rlv: row
                    .get(3)
                    .unwrap()
                    .strip_prefix("RecipeLevelTable#")
                    .unwrap()
                    .parse()
                    .unwrap(),
                name: row.get(4).unwrap().into(),
                job: row.get(2).unwrap().into(),
                difficulty_factor: row.get(29).unwrap().parse().unwrap(),
                quality_factor: row.get(30).unwrap().parse().unwrap(),
                durability_factor: row.get(31).unwrap().parse().unwrap(),
            })
        })
        .collect::<Result<Vec<_>, csv::Error>>()
        .unwrap()
}

struct AppState {
    solver_list: Mutex<HashMap<solver::SolverHash, Box<solver::Solver>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            solver_list: Mutex::new(HashMap::new()),
        }
    }
}

#[tauri::command(async)]
fn create_solver(
    status: Status,
    synth_skills: Vec<Skills>,
    touch_skills: Vec<Skills>,
    app_state: tauri::State<AppState>,
) -> Result<(), String> {
    let key = solver::SolverHash {
        attributes: status.attributes,
        recipe: status.recipe,
    };
    let list = &mut *app_state.solver_list.lock().unwrap();
    match list.entry(key) {
        Entry::Occupied(_) => Err("solver already exists".to_string()),
        Entry::Vacant(e) => {
            let mut driver = solver::Driver::new(&status);
            driver.init(&synth_skills);
            let mut solver = solver::Solver::new(driver);
            solver.init(&touch_skills);
            e.insert(Box::new(solver));
            Ok(())
        }
    }
}

#[tauri::command(async)]
fn read_solver(status: Status, app_state: tauri::State<AppState>) -> Result<Vec<Skills>, String> {
    let key = solver::SolverHash {
        attributes: status.attributes,
        recipe: status.recipe,
    };
    let list = &mut *app_state.solver_list.lock().unwrap();
    match list.entry(key) {
        Entry::Occupied(e) => {
            let solver = e.get().read_all(&status);
            Ok(solver.1)
        }
        Entry::Vacant(_) => Err("solver not exists".to_string()),
    }
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            new_recipe,
            new_status,
            simulate,
            recipe_table,
            create_solver,
            read_solver,
        ])
        .run(tauri::generate_context!())
        .map_err(|err| {
            msgbox::create(
                "错误",
                format!("error while running tauri application: {}", err).as_str(),
                msgbox::IconType::Error,
            )
        })
        .unwrap();
}
