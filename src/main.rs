#[macro_use]
extern crate lazy_static;

use serde::{Serialize, Deserialize};
use std::io::Write;
use std::fs;
use std::env;
use std::cmp::Ordering;
use std::process;
use std::process::{Command, Stdio};
use std::path::Path;
use std::collections::HashMap;
use std::time::SystemTime;

#[derive(Serialize, Deserialize)]
struct HistoryEntry {
    rank:   u32,
    access: u64
}

lazy_static! {
    static ref INDEX_PATH: String = env::var("AppData").unwrap() + "/wlines_run_index.json";
    static ref HISTORY_PATH: String = env::var("AppData").unwrap() + "/wlines_run_history.json";
}

const EXTENSIONS: &'static [&'static str] = &[
    "exe",
    "lnk",
    "bat",
    "cmd",
    "com"
];

fn frecency(history: &HistoryEntry, current_time: u64) -> f64 {
    (history.rank as f64) / (((current_time as f64) - (history.access as f64)).sqrt() / 10.0 + 5.0)
}

fn index_directory(programs: &mut HashMap<String, String>, dir: &Path, prefix: &Path, recursively: bool) {
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd {
            let path = entry.unwrap().path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if EXTENSIONS.iter().any(|&e| e == ext) {
                        programs.insert(
                            String::from(path.strip_prefix(prefix).unwrap().to_str().unwrap()),
                            String::from(path.to_str().unwrap())
                        );
                    }
                }
            } else if path.is_dir() && recursively {
                let _ = index_directory(programs, &path, prefix, true);
            }
        }
    }
}

fn index_start_menu(programs: &mut HashMap<String, String>) {
    const PROG_DIR: &'static str = "/Microsoft/Windows/Start Menu/Programs";

    let path_str = env::var("AppData").unwrap() + PROG_DIR;
    let path = Path::new(&path_str);
    index_directory(programs, path, path, true);

    let path_str = env::var("ProgramData").unwrap() + PROG_DIR;
    let path = Path::new(&path_str);
    index_directory(programs, path, path, true);
}

fn index_path(programs: &mut HashMap<String, String>) {
    for path in env::split_paths(&env::var("PATH").unwrap()) {
        index_directory(programs, &path, &path, false);
    }
}

fn cmd_index() {
    let mut programs: HashMap<String, String> = HashMap::new();
    index_start_menu(&mut programs);
    index_path(&mut programs);

    let index_json_data = serde_json::to_string_pretty(&programs).unwrap();
    fs::write(&*INDEX_PATH, index_json_data)
        .expect("Unable to write to wlines_run_index.json");
    println!("Indexed {} programs", programs.len());
}

fn cmd_run(wlines_args: Vec<String>) {
    // Start wlines right away
    let mut wlines = Command::new("wlines")
        .args(wlines_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Couldn't start wlines");

    // Load index
    let index_json_data = fs::read_to_string(&*INDEX_PATH)
        .expect("Unable to load wlines_run_index.json");
    let programs: HashMap<String, String> = serde_json::from_str(&index_json_data).unwrap();
    println!("Loaded {} indexed programs", programs.len());

    // Load history
    let mut history: HashMap<String, HistoryEntry>;
    if let Ok(history_json_data) = fs::read_to_string(&*HISTORY_PATH) {
        history = serde_json::from_str(&history_json_data).unwrap();
        println!("Loaded history");
    } else {
        history = HashMap::new();
    }

    // Sort programs by frecency
    let time_now: u64 = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let mut prog_names: Vec<&String> = programs.keys().collect();
    prog_names.sort_by(|a, b| {
        if history.contains_key(*a) && history.contains_key(*b) {
            let a_score = frecency(&history[*a], time_now);
            let b_score = frecency(&history[*b], time_now);
            b_score.partial_cmp(&a_score).unwrap()
        } else if history.contains_key(*a) {
            Ordering::Less
        } else if history.contains_key(*b) {
            Ordering::Greater
        } else {
            a.cmp(&b)
        }
    });

    // Send to wlines
    {
        let prog_names_str = prog_names
            .iter()
            .fold(String::new(), |acc, prog_name| acc + "\"" + prog_name + "\"\n");
        let stdin = wlines.stdin.as_mut().unwrap();
        stdin.write_all(prog_names_str.as_bytes())
            .expect("Couldn't communicate with wlines stdin");
    }

    // Wait for output
    let output = wlines.wait_with_output().expect("Failed to read wlines output");
    if !output.status.success() {
        println!("Exited\n");
        return;
    }

    // Match selection
    let input_string = String::from_utf8(output.stdout).unwrap().trim().to_string();
    let matched_input = prog_names
        .iter()
        .find(|&&a| input_string.starts_with(&format!("\"{}\"", a)));
    let choice = if let Some(x) = matched_input { x } else {
        println!("Unknown choice\n");
        return;
    };
    let program_path = programs.get(*choice).unwrap();

    // Launch it
    // todo: add argument functionality
    println!("Starting \"{}\"\n", program_path);
    Command::new("cmd")
        .args(&["/c", program_path])
        .spawn()
        .expect("Couldn't start program");

    // Save to history
    match history.get_mut(*choice) {
        Some(entry) => {
            entry.rank += 1;
            entry.access = time_now;
        },
        None => {
            history.insert(choice.to_string(), HistoryEntry {
                rank: 1,
                access: time_now
            });
        }
    }

    let history_json_data = serde_json::to_string_pretty(&history).unwrap();
    fs::write(&*HISTORY_PATH, history_json_data)
        .expect("Unable to write to wlines_run_history.json");
}

fn usage() {
    eprintln!("wlines_run <index|run [args...]>");
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        usage();
    }

    if args[1] == "index" {
        cmd_index();
    } else if args[1] == "run" {
        cmd_run(args[2..].to_vec());
    } else {
        usage();
    }
}
