#[macro_use]
extern crate lazy_static;

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process;
use std::process::{Command, Stdio};
use std::time::SystemTime;

#[derive(Serialize, Deserialize)]
struct HistoryEntry {
    rank: u32,
    access: u64,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
enum SourceType {
    StartMenu,
    Path,
    // todo: WinApp,
}

impl SourceType {
    fn display_name(&self) -> &'static str {
        match *self {
            SourceType::StartMenu => "S",
            SourceType::Path => "P",
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Program {
    title: String,
    source: SourceType,
    abs_path: String,
}

lazy_static! {
    static ref INDEX_PATH: String = env::var("AppData").unwrap() + "/wlines_run_index.json";
    static ref HISTORY_PATH: String = env::var("AppData").unwrap() + "/wlines_run_history.json";
}

const EXTENSIONS: &'static [&'static str] = &["exe", "lnk", "bat", "cmd", "com"];

fn frecency(history: &HistoryEntry, current_time: u64) -> f64 {
    (history.rank as f64) / (((current_time as f64) - (history.access as f64)).sqrt() / 10.0 + 5.0)
}

fn index_directory(
    programs: &mut HashMap<String, Program>,
    dir: &Path,
    prefix: &Path,
    source: &SourceType,
    recursively: bool,
) {
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd {
            let path = entry.unwrap().path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if EXTENSIONS.iter().any(|&e| e == ext) {
                        let title_str =
                            String::from(path.strip_prefix(prefix).unwrap().to_str().unwrap());
                        let path_str = String::from(path.to_str().unwrap());
                        programs.insert(
                            path_str.to_ascii_lowercase(),
                            Program {
                                title: title_str,
                                abs_path: path_str,
                                source: *source,
                            },
                        );
                    }
                }
            } else if path.is_dir() && recursively {
                let _ = index_directory(programs, &path, prefix, source, true);
            }
        }
    }
}

fn index_start_menu(programs: &mut HashMap<String, Program>) {
    const PROG_DIR: &'static str = "/Microsoft/Windows/Start Menu/Programs";

    let path_str = env::var("AppData").unwrap() + PROG_DIR;
    let path = Path::new(&path_str);
    index_directory(programs, path, path, &SourceType::StartMenu, true);

    let path_str = env::var("ProgramData").unwrap() + PROG_DIR;
    let path = Path::new(&path_str);
    index_directory(programs, path, path, &SourceType::StartMenu, true);
}

fn index_path(programs: &mut HashMap<String, Program>) {
    for path in env::split_paths(&env::var("PATH").unwrap()) {
        index_directory(programs, &path, &path, &SourceType::Path, false);
    }
}

fn cmd_index() {
    // When indexing, use lowercase program path as a key to prevent some duplicates
    let mut programs: HashMap<String, Program> = HashMap::new();
    index_start_menu(&mut programs);
    index_path(&mut programs);

    // Collect into vector since we don't need the dictionary structure anymore
    let programs: Vec<&Program> = programs.values().collect();

    // Write to file
    let index_json_data = serde_json::to_string_pretty(&programs).unwrap();
    fs::write(&*INDEX_PATH, index_json_data).expect("Unable to write to wlines_run_index.json");
    println!("Indexed {} programs", programs.len());
}

fn format_program_display_name(program: &Program) -> String {
    format!("{}] ", program.source.display_name()) + &program.title
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
    let index_json_data =
        fs::read_to_string(&*INDEX_PATH).expect("Unable to load wlines_run_index.json");
    let mut programs: Vec<Program> = serde_json::from_str(&index_json_data).unwrap();
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
    let time_now: u64 = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    programs.sort_by(|a, b| {
        if history.contains_key(&a.abs_path) && history.contains_key(&b.abs_path) {
            let a_score = frecency(&history[&a.abs_path], time_now);
            let b_score = frecency(&history[&b.abs_path], time_now);
            b_score.partial_cmp(&a_score).unwrap()
        } else if history.contains_key(&a.abs_path) {
            Ordering::Less
        } else if history.contains_key(&b.abs_path) {
            Ordering::Greater
        } else {
            a.title.cmp(&b.title)
        }
    });

    // Create display names for each program that link back to each program
    let prog_name_links: Vec<(String, &Program)> = programs
        .iter()
        .map(|program| (format_program_display_name(program), program))
        .collect();

    // Send to wlines
    {
        let prog_names_str = prog_name_links
            .iter()
            .fold(String::new(), |acc, prog_name_link| {
                acc + &prog_name_link.0 + ": \n"
            });

        let stdin = wlines.stdin.as_mut().unwrap();
        stdin
            .write_all(prog_names_str.as_bytes())
            .expect("Couldn't communicate with wlines stdin");
    }

    // Wait for output
    // todo: allow for multiple inputs from the same menu (ctrl+enter)
    let output = wlines
        .wait_with_output()
        .expect("Failed to read wlines output");
    if !output.status.success() {
        println!("Exited\n");
        return;
    }

    // Match selection
    let input_string = String::from_utf8(output.stdout).unwrap().trim().to_string();
    let matched_input = prog_name_links
        .iter()
        .find(|&prog_name_link| input_string.starts_with(&format!("{}:", prog_name_link.0)));
    let chosen_prog = if let Some(x) = matched_input {
        x
    } else {
        println!("Unknown choice '{}'\n", input_string);
        return;
    };

    // Extract input arguments
    let mut prog_args: Vec<String> = Vec::new();
    if input_string.len() > chosen_prog.0.len() + 1 {
        // + 1 to compensate `:` suffix
        let arg_string = input_string[(chosen_prog.0.len() + 1)..].to_string();
        prog_args = shlex::split(&arg_string).unwrap();
    }

    // Launch it
    println!("Starting \"{}\"\n", chosen_prog.1.abs_path);
    let mut launch_args: Vec<String> = vec![
        String::from("/c"),
        String::from("start"),
        String::from(""),
        chosen_prog.1.abs_path.clone(),
    ];
    launch_args.append(&mut prog_args);
    Command::new("cmd")
        .args(launch_args)
        .spawn()
        .expect("Couldn't start program");

    // Save to history
    match history.get_mut(&chosen_prog.1.abs_path) {
        Some(entry) => {
            entry.rank += 1;
            entry.access = time_now;
        }
        None => {
            history.insert(
                chosen_prog.1.abs_path.to_string(),
                HistoryEntry {
                    rank: 1,
                    access: time_now,
                },
            );
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
