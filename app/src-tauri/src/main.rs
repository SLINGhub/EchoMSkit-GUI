#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod project_setup;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

struct RunState(AtomicBool);

struct RunGuard<'a>(&'a AtomicBool);

impl RunState {
    fn start(&self) -> Result<RunGuard<'_>, String> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "EchoMSkit is already running".to_string())?;
        Ok(RunGuard(&self.0))
    }
}

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

const BOUNDS_BACKUP_DIR: &str = "acq_time_backups";
const ORIGINAL_BOUNDS_BACKUP: &str = "acq_time_original.csv";
const ACQ_TIME: &str = "acq_time.csv";
const RESULTS: &str = "EchoMSkit_results.csv";
const DATA_DIR: &str = "wiff_dir";
const DEFAULT_SAMPLE_LIST: &str = "sample_list.csv";
const DEFAULT_TRANSITIONS: &str = "transitions.csv";
const DETECT_STEP: (&str, &str) = ("1", "sample window detection");
const AREA_STEP: (&str, &str) = ("2only", "area calculation and gui plot data");

#[derive(Default, Deserialize, Serialize)]
struct Settings {
    #[serde(default)]
    last_project: String,
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    last_backups: HashMap<String, String>,
    #[serde(default)]
    backup_labels: HashMap<String, HashMap<String, String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectOutputs {
    acq_time: bool,
    results_csv: bool,
    misc_data: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DataEditorFile {
    kind: String,
    title: String,
    name: String,
    format: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DataEditorContent {
    file: DataEditorFile,
    text: Option<String>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    backups: Vec<String>,
    note: Option<String>, // shown instead of "Ready", e.g. for a file that is not saved yet
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataEditorSaveInput {
    kind: String,
    text: Option<String>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    name: String,
    path: String,
    data_count: usize,
    transition_count: usize,
    transition_file: Option<String>,
    worker_name: Option<String>,
    issues: Vec<String>,
    outputs: ProjectOutputs,
    can_run: bool,
    setup: Vec<String>, // what project_setup changed on this scan, for the activity log
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualizerPoint {
    rt: f32,
    intensity: f32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualizerWindow {
    label: String,
    sample_id: Option<String>,
    start_index: usize,
    end_index: usize,
    rt_start: f32,
    rt_end: f32,
    apex_rt: f32,
    height: f32,
    area: f32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualizerSample {
    sample_id: String,
    display_name: String,
    points: Vec<VisualizerPoint>,
    wells: Vec<VisualizerWindow>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VisualizerTransitionData {
    transition: String,
    samples: Vec<VisualizerSample>,
    sample_count: usize,
    well_count: usize,
    global_rt_min: f32,
    global_rt_max: f32,
    global_intensity_max: f32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoundEdit {
    sample_id: String,
    well_label: String,
    rt_start: f32,
    rt_end: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupList {
    backups: Vec<String>,
    last: Option<String>,
    labels: HashMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveBoundsResult {
    written: usize,
    backup: Option<String>,
    project: ProjectSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupState {
    project: Option<ProjectSummary>,
    remembered_path: Option<String>,
    needs_reselection: bool,
    theme: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkerOutput {
    stream: String,
    line: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunStatus {
    status: String,
}

struct BinReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BinReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| "the visualizer data file is too large".to_string())?;
        if end > self.data.len() {
            return Err("the visualizer data file ended unexpectedly".to_string());
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn skip(&mut self, len: usize) -> Result<(), String> {
        self.take(len).map(|_| ())
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| "the visualizer data file is malformed".to_string())?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| "the visualizer data file is malformed".to_string())?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32, String> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| "the visualizer data file is malformed".to_string())?;
        Ok(f32::from_le_bytes(bytes))
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != 0 {
            self.pos += 1;
        }
        if self.pos >= self.data.len() {
            return Err("the visualizer data string terminator was missing".to_string());
        }
        let bytes = &self.data[start..self.pos];
        self.pos += 1;
        String::from_utf8(bytes.to_vec()).map_err(|error| error.to_string())
    }
}

// stores gui settings in the app config folder
fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join("settings.json"))
        .map_err(|error| error.to_string())
}

// reads saved gui settings if present
fn read_settings(app: &AppHandle) -> Option<Settings> {
    let contents = fs::read(settings_path(app).ok()?).ok()?;
    serde_json::from_slice(&contents).ok()
}

// writes gui settings atomically enough for small files
fn write_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let settings_path = settings_path(app)?;
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    fs::write(settings_path, json).map_err(|error| error.to_string())
}

// remembers the last selected project
fn save_project(app: &AppHandle, project: &Path) -> Result<(), String> {
    let mut settings = read_settings(app).unwrap_or_default();
    settings.last_project = project.to_string_lossy().into_owned();
    write_settings(app, &settings)
}

// appends .exe only on windows
fn worker_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn is_file(path: &Path) -> bool {
    path.is_file()
}

// finds the repository root in dev builds
fn source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

// lists worker locations from most to least portable
fn candidate_worker_paths(project: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("ECHOMSKIT_WORKER").map(PathBuf::from) {
        candidates.push(path);
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        candidates.push(directory.join(worker_name("EchoMSkit-worker")));
        candidates.push(directory.join(worker_name("EchoMSkit")));
    }

    let root = source_root();
    candidates.push(
        root.join("target")
            .join("release")
            .join(worker_name("EchoMSkit")),
    );
    candidates.push(
        root.join("target")
            .join("debug")
            .join(worker_name("EchoMSkit")),
    );

    candidates.push(project.join(worker_name("EchoMSkit")));
    candidates.push(project.join(if cfg!(windows) {
        "EchoMSkit_windows.exe"
    } else {
        "EchoMSkit_macOS"
    }));
    candidates
}

fn find_worker(project: &Path) -> Option<PathBuf> {
    candidate_worker_paths(project)
        .into_iter()
        .find(|path| is_file(path))
}

// resolves param.txt paths against the dataset folder
fn project_file(project: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        project.join(path)
    }
}

// shows paths relative to the dataset when possible
fn display_name(project: &Path, path: &Path) -> String {
    path.strip_prefix(project)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

// rejects backup names that could leave the backup folder
fn safe_backup_name(name: &str) -> Result<&str, String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || !name.ends_with(".csv")
    {
        return Err("the backup name is invalid".to_string());
    }
    Ok(name)
}

fn writable_bounds_backup_name(name: &str) -> Result<&str, String> {
    let name = safe_backup_name(name)?;
    if name.eq_ignore_ascii_case(ORIGINAL_BOUNDS_BACKUP) {
        return Err("Original acq_time.csv cannot be overwritten".to_string());
    }
    Ok(name)
}

// keeps renamed backup labels small and printable
fn safe_backup_label(label: &str) -> Result<String, String> {
    let label = label.trim();
    if label.is_empty() {
        return Err("the backup label cannot be blank".to_string());
    }
    if label.chars().count() > 80 || label.chars().any(char::is_control) {
        return Err("the backup label is too long or contains invalid characters".to_string());
    }
    Ok(label.to_string())
}

// keeps original at the top of the backup menu
fn order_backups(mut backups: Vec<String>) -> Vec<String> {
    backups.sort_unstable();
    if let Some(index) = backups
        .iter()
        .position(|name| name == ORIGINAL_BOUNDS_BACKUP)
    {
        let original = backups.remove(index);
        backups.insert(0, original);
    }
    backups
}

// saves the first untouched acq_time.csv
fn capture_original_bounds(project: &Path) -> Result<(), String> {
    let source = project.join(ACQ_TIME);
    if !source.is_file() {
        return Ok(());
    }
    let directory = project.join(BOUNDS_BACKUP_DIR);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::copy(source, directory.join(ORIGINAL_BOUNDS_BACKUP))
        .map(|_| ())
        .map_err(|error| error.to_string())
}

// creates the protected original backup if missing
fn ensure_original_bounds(project: &Path) -> Result<(), String> {
    if !project
        .join(BOUNDS_BACKUP_DIR)
        .join(ORIGINAL_BOUNDS_BACKUP)
        .is_file()
    {
        capture_original_bounds(project)?;
    }
    Ok(())
}

// copies current acq_time.csv into a named backup
fn backup_bounds(project: &Path, name: &str) -> Result<Option<String>, String> {
    let name = writable_bounds_backup_name(name)?;
    let source = project.join(ACQ_TIME);
    if !source.is_file() {
        return Ok(None);
    }
    let directory = project.join(BOUNDS_BACKUP_DIR);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::copy(&source, directory.join(name)).map_err(|error| error.to_string())?;
    Ok(Some(name.to_string()))
}

// mirrors the worker's param.txt checks so problems show before a run
fn parameter_issues(parameters: &toml::Table) -> Vec<String> {
    let positive = |key: &str| {
        parameters.get(key).map(|value| {
            value
                .as_float()
                .or_else(|| value.as_integer().map(|value| value as f64))
                .is_some_and(|value| value.is_finite() && value > 0.0)
        })
    };
    let mut issues = Vec::new();
    if positive("sample_duration") != Some(true) {
        issues
            .push("sample_duration must be a positive number of seconds in param.txt".to_string());
    }
    if positive("mass_tolerance") == Some(false) {
        issues.push("mass_tolerance must be a positive number of Da in param.txt".to_string());
    }
    issues
}

// shows acquisitions without the .wiff extension
fn sample_display_name(file: &str) -> String {
    let path = Path::new(file);
    match (path.extension(), path.file_stem()) {
        (Some(extension), Some(stem)) if extension.eq_ignore_ascii_case("wiff") => {
            stem.to_string_lossy().into_owned()
        }
        _ => file.to_string(),
    }
}

// sample list files match with or without the extension
fn file_key(file: &str) -> String {
    sample_display_name(file.trim()).to_lowercase()
}

// the ejection number that starts window labels like "3 B2"
fn window_index(label: &str) -> Option<u32> {
    label.split_whitespace().next()?.parse().ok()
}

// normalizes headers for small csv lookups
fn normalized_header(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

// finds the sample list in the dataset folder
fn find_sample_list(project: &Path) -> Option<PathBuf> {
    let exact = project.join(DEFAULT_SAMPLE_LIST);
    if exact.is_file() {
        return Some(exact);
    }
    find_named_csv(project, &["sample", "list"])
}

// sample ids keyed by acquisition and ejection, rows without a file apply to every acquisition
#[derive(Default)]
struct SampleIds {
    by_file: HashMap<(String, u32), String>,
    any_file: HashMap<u32, String>,
}

impl SampleIds {
    fn get(&self, file: &str, label: &str) -> Option<String> {
        let index = window_index(label)?;
        self.by_file
            .get(&(file_key(file), index))
            .or_else(|| self.any_file.get(&index))
            .cloned()
    }
}

// reads File, Index and Sample_ID columns from the sample list
fn sample_ids(project: &Path) -> SampleIds {
    let mut ids = SampleIds::default();
    let Some(path) = find_sample_list(project) else {
        return ids;
    };
    let Ok((headers, rows)) = read_csv_table(&path, &[]) else {
        return ids;
    };
    let headers = headers
        .iter()
        .map(|cell| normalized_header(cell))
        .collect::<Vec<_>>();
    let index = |names: &[&str]| {
        headers
            .iter()
            .position(|header| names.contains(&header.as_str()))
    };
    let (Some(index_column), Some(sample_column)) = (
        index(&["index", "ejection", "injection"]),
        index(&["sampleid"]),
    ) else {
        return ids;
    };
    let file_column = index(&["file", "datafile", "wiff"]);

    for cells in rows {
        let cell = |column: usize| cells.get(column).map_or("", |cell| cell.trim());
        let Ok(number) = cell(index_column).parse::<u32>() else {
            continue;
        };
        let sample_id = cell(sample_column);
        if sample_id.is_empty() {
            continue;
        }
        match file_column.map(cell).filter(|file| !file.is_empty()) {
            Some(file) => ids
                .by_file
                .insert((file_key(file), number), sample_id.to_string()),
            None => ids.any_file.insert(number, sample_id.to_string()),
        };
    }
    ids
}

// writes dragged bounds into acq_time.csv, keeping its sample duration tag
fn save_bounds_to_acq_time(project: &Path, edits: &[BoundEdit]) -> Result<usize, String> {
    if edits.is_empty() {
        return Ok(0);
    }
    let path = project.join(ACQ_TIME);
    let contents =
        fs::read_to_string(&path).map_err(|_| "acq_time.csv was not found".to_string())?;
    let (tag, body) = match contents.split_once('\n') {
        Some((first, rest)) if first.starts_with('#') => (Some(first.trim_end()), rest),
        _ => (None, contents.as_str()),
    };
    let mut rows = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(body.as_bytes())
        .records()
        .map(|record| record.map(|record| record.iter().map(str::to_string).collect::<Vec<_>>()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let mut windows = HashMap::new();
    let mut file = None;
    for (index, row) in rows.iter().enumerate() {
        if row.len() < 3 {
            continue;
        }
        if row[1].trim().is_empty() && row[2].trim().is_empty() {
            file = Some(row[0].trim().to_string());
        } else if let Some(file) = &file {
            windows.insert((file.clone(), row[0].trim().to_string()), index);
        }
    }

    for edit in edits {
        let index = *windows
            .get(&(edit.sample_id.clone(), edit.well_label.clone()))
            .ok_or_else(|| {
                format!(
                    "sample {} in {} was not found in acq_time.csv",
                    edit.well_label, edit.sample_id
                )
            })?;
        let (low, high) = if edit.rt_start <= edit.rt_end {
            (edit.rt_start, edit.rt_end)
        } else {
            (edit.rt_end, edit.rt_start)
        };
        if !low.is_finite() || !high.is_finite() {
            return Err("integration bounds must be finite".to_string());
        }
        rows[index][1] = format!("{low:.3}");
        rows[index][2] = format!("{high:.3}");
    }

    let mut writer = csv::WriterBuilder::new()
        .flexible(true)
        .from_writer(Vec::new());
    for row in &rows {
        writer
            .write_record(row)
            .map_err(|error| error.to_string())?;
    }
    let table = writer.into_inner().map_err(|error| error.to_string())?;
    let mut output = tag.map_or_else(Vec::new, |tag| format!("{tag}\n").into_bytes());
    output.extend(table);
    fs::write(path, output).map_err(|error| error.to_string())?;
    Ok(edits.len())
}

// writes EchoMSkit_results.csv from the worker long.csv output
fn write_labeled_results(project: &Path) -> Result<(), String> {
    let source = project.join("long.csv");
    if !source.is_file() {
        return Ok(());
    }
    let results = project.join(RESULTS);
    let (mut header, rows) = read_csv_table(&source, &[])?;
    if header.is_empty() {
        fs::write(results, "").map_err(|error| error.to_string())?;
        return Ok(());
    }

    let sample_ids = sample_ids(project);
    let headers = header
        .iter()
        .map(|cell| normalized_header(cell))
        .collect::<Vec<_>>();
    let sample_index = headers
        .iter()
        .position(|header| header == "sample")
        .unwrap_or(0);
    let well_index = headers
        .iter()
        .position(|header| header == "well")
        .unwrap_or(2);
    let insert_index = 3.min(header.len());
    header.insert(insert_index, "Sample_ID".to_string());

    let mut output_rows = Vec::new();
    for mut cells in rows {
        let label = match (cells.get(sample_index), cells.get(well_index)) {
            (Some(file), Some(well)) => sample_ids.get(file, well).unwrap_or_default(),
            _ => String::new(),
        };
        while cells.len() < insert_index {
            cells.push(String::new());
        }
        cells.insert(insert_index, label);
        output_rows.push(cells);
    }

    write_csv_table(&results, &header, &output_rows)
}

// removes long.csv when the gui only needed it as a temporary worker output
fn cleanup_generated_long_csv(project: &Path, existed_before: bool) -> Result<(), String> {
    if existed_before || !project.join(RESULTS).is_file() {
        return Ok(());
    }
    match fs::remove_file(project.join("long.csv")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

// uses the labeled result table once it exists
fn preferred_results_file(project: &Path) -> PathBuf {
    let labeled = project.join(RESULTS);
    if labeled.is_file() {
        labeled
    } else {
        project.join("long.csv")
    }
}

// returns a compact timestamp for editor backup names
fn timestamp_token() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| error.to_string())
}

// reads param.txt without making it required for editor detection
fn project_parameters(project: &Path) -> Option<toml::Table> {
    fs::read_to_string(project.join("param.txt"))
        .ok()?
        .parse::<toml::Table>()
        .ok()
}

// finds a root-level csv containing all requested words
fn find_named_csv(project: &Path, words: &[&str]) -> Option<PathBuf> {
    let mut candidates = fs::read_dir(project)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_ascii_lowercase())
                    .is_some_and(|name| {
                        name.ends_with(".csv") && words.iter().all(|word| name.contains(word))
                    })
        })
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.into_iter().next()
}

// resolves the transition list from param.txt or the folder
fn transition_table(project: &Path) -> PathBuf {
    project_parameters(project)
        .and_then(|parameters| {
            parameters
                .get("transition_list")
                .and_then(toml::Value::as_str)
                .map(|value| project_file(project, value))
        })
        .or_else(|| find_named_csv(project, &["transition"]))
        .unwrap_or_else(|| project.join(DEFAULT_TRANSITIONS))
}

// returns editor file metadata for echomskit inputs
fn data_editor_files_for_project(project: &Path) -> Vec<DataEditorFile> {
    let sample_list =
        find_sample_list(project).unwrap_or_else(|| project.join(DEFAULT_SAMPLE_LIST));
    let transition = transition_table(project);
    [
        ("param", "param.txt", project.join("param.txt"), "text"),
        ("sampleList", "Sample list", sample_list, "csv"),
        ("transition", "Transition list", transition, "csv"),
    ]
    .into_iter()
    .map(|(kind, title, path, format)| DataEditorFile {
        kind: kind.to_string(),
        title: title.to_string(),
        name: display_name(project, &path),
        format: format.to_string(),
    })
    .collect()
}

// resolves a single editor file by kind
fn data_editor_file(project: &Path, kind: &str) -> Result<DataEditorFile, String> {
    data_editor_files_for_project(project)
        .into_iter()
        .find(|file| file.kind == kind)
        .ok_or_else(|| "that editable file is not available".to_string())
}

// resolves the absolute path for a single editor file
fn data_editor_path(project: &Path, kind: &str) -> Result<PathBuf, String> {
    match kind {
        "param" => Ok(project.join("param.txt")),
        "sampleList" => {
            Ok(find_sample_list(project).unwrap_or_else(|| project.join(DEFAULT_SAMPLE_LIST)))
        }
        "transition" => Ok(transition_table(project)),
        _ => Err("that editable file is not available".to_string()),
    }
}

// returns the backup folder for an editable file
fn data_editor_backup_dir(project: &Path, kind: &str) -> Result<PathBuf, String> {
    data_editor_file(project, kind)?;
    Ok(project.join("data_file_backups").join(kind))
}

// names the protected original backup
fn data_editor_original_backup_name(project: &Path, kind: &str) -> Result<String, String> {
    let path = data_editor_path(project, kind)?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("txt");
    Ok(format!("original.{extension}"))
}

// creates the protected original backup once
fn ensure_data_editor_original_backup(
    project: &Path,
    kind: &str,
) -> Result<Option<String>, String> {
    let path = data_editor_path(project, kind)?;
    if !path.is_file() {
        return Ok(None);
    }
    let name = data_editor_original_backup_name(project, kind)?;
    let directory = data_editor_backup_dir(project, kind)?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let target = directory.join(&name);
    if !target.is_file() {
        fs::copy(path, target).map_err(|error| error.to_string())?;
    }
    Ok(Some(name))
}

// lists saved editor backups with original first
fn list_data_editor_backups(project: &Path, kind: &str) -> Result<Vec<String>, String> {
    let original = ensure_data_editor_original_backup(project, kind)?;
    let directory = data_editor_backup_dir(project, kind)?;
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut backups = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect::<Vec<_>>();
    backups.sort_unstable();
    if let Some(original) = original
        && let Some(index) = backups.iter().position(|name| name == &original)
    {
        let original = backups.remove(index);
        backups.insert(0, original);
    }
    Ok(backups)
}

// reads a csv table into headers and rows
fn read_csv_table(
    path: &Path,
    fallback_headers: &[&str],
) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    if !path.is_file() {
        return Ok((
            fallback_headers
                .iter()
                .map(|header| header.to_string())
                .collect(),
            Vec::new(),
        ));
    }
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(contents.as_bytes());
    let mut records = reader.records();
    let headers = records
        .next()
        .transpose()
        .map_err(|error| error.to_string())?
        .map(|record| record.iter().map(str::to_string).collect())
        .unwrap_or_else(|| {
            fallback_headers
                .iter()
                .map(|header| header.to_string())
                .collect()
        });
    let rows = records
        .map(|record| record.map(|record| record.iter().map(str::to_string).collect()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok((headers, rows))
}

// writes a csv table from editor rows
fn write_csv_table(path: &Path, headers: &[String], rows: &[Vec<String>]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut writer = csv::WriterBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|error| error.to_string())?;
    writer
        .write_record(headers)
        .map_err(|error| error.to_string())?;
    for row in rows {
        writer
            .write_record(row)
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

// reads current or backup editor content
fn read_data_editor_content_from_path(
    project: &Path,
    kind: &str,
    path: &Path,
) -> Result<DataEditorContent, String> {
    let file = data_editor_file(project, kind)?;
    let backups = list_data_editor_backups(project, kind)?;
    if file.format == "text" {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.to_string()),
        };
        return Ok(DataEditorContent {
            file,
            text: Some(text),
            headers: Vec::new(),
            rows: Vec::new(),
            backups,
            note: None,
        });
    }
    if kind == "sampleList" && !path.is_file() {
        let rows = sample_list_template(project);
        let note = if rows.is_empty() {
            "Not saved yet, run EchoMSkit to list the detected windows here"
        } else {
            "Not saved yet, rows come from the detected windows"
        };
        return Ok(DataEditorContent {
            file,
            text: None,
            headers: ["File", "Index", "Well", "Sample_ID"]
                .map(String::from)
                .to_vec(),
            rows,
            backups,
            note: Some(note.to_string()),
        });
    }
    let fallback = match kind {
        "transition" => vec!["Name", "Q1", "Q3", "XIC Width"],
        _ => Vec::new(),
    };
    let (headers, rows) = read_csv_table(path, &fallback)?;
    Ok(DataEditorContent {
        file,
        text: None,
        headers,
        rows,
        backups,
        note: None,
    })
}

/* one row per window in acq_time.csv with the sample id left blank, so a new
sample list only needs the ids typed in. the well column is just for orientation,
sample_ids matches columns by header and ignores it */
fn sample_list_template(project: &Path) -> Vec<Vec<String>> {
    let Ok(contents) = fs::read_to_string(project.join(ACQ_TIME)) else {
        return Vec::new();
    };
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .comment(Some(b'#'))
        .from_reader(contents.as_bytes());
    let mut file = String::new();
    let mut rows = Vec::new();
    for record in reader.records().filter_map(Result::ok) {
        let label = record.get(0).unwrap_or("").trim();
        if record.get(1).is_none_or(str::is_empty) {
            file = label.to_string();
            continue;
        }
        let Some(index) = window_index(label) else {
            continue;
        };
        let well = label.split_once(' ').map_or("", |(_, well)| well.trim());
        rows.push(vec![
            file.clone(),
            index.to_string(),
            well.to_string(),
            String::new(),
        ]);
    }
    rows
}

// validates editor backup file names
fn safe_data_editor_backup_name(name: &str, old_name: &str) -> Result<String, String> {
    let mut name = name.trim().to_string();
    if name.is_empty()
        || name.len() > 120
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name
            .chars()
            .any(|character| matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        || name.chars().any(|character| character.is_control())
    {
        return Err("the backup name is invalid".to_string());
    }
    if Path::new(&name).extension().is_none()
        && let Some(extension) = Path::new(old_name)
            .extension()
            .and_then(|value| value.to_str())
    {
        name.push('.');
        name.push_str(extension);
    }
    Ok(name)
}

// backs up the current editable file before save or restore
fn backup_data_editor_file(
    project: &Path,
    kind: &str,
    path: &Path,
) -> Result<Option<String>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let directory = data_editor_backup_dir(project, kind)?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("txt");
    let timestamp = timestamp_token()?;
    let mut suffix = 0_u64;
    let mut source = fs::File::open(path).map_err(|error| error.to_string())?;
    loop {
        let name = if suffix == 0 {
            format!("{kind}_{timestamp}.{extension}")
        } else {
            format!("{kind}_{timestamp}_{suffix}.{extension}")
        };
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(&name))
        {
            Ok(mut backup) => {
                std::io::copy(&mut source, &mut backup).map_err(|error| error.to_string())?;
                backup.flush().map_err(|error| error.to_string())?;
                return Ok(Some(name));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => suffix += 1,
            Err(error) => return Err(error.to_string()),
        }
    }
}

#[derive(Default)]
struct DataFiles {
    wiff: usize,
    missing_scans: Vec<String>,
    // .wiff2 without the readable .wiff next to it
    lone_wiff2: Vec<String>,
}

// counts .wiff files in wiff_dir and lists the ones the worker cannot read
fn data_files(directory: &Path) -> DataFiles {
    let mut files = DataFiles::default();
    let Ok(entries) = fs::read_dir(directory) else {
        return files;
    };
    let mut stems = Vec::new();
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("wiff") && path.is_file() => {
                files.wiff += 1;
                stems.push(name[..name.len() - ".wiff".len()].to_ascii_lowercase());
                let mut scan = path.clone().into_os_string();
                scan.push(".scan");
                if !Path::new(&scan).is_file() {
                    files.missing_scans.push(name.to_string());
                }
            }
            Some(extension) if extension.eq_ignore_ascii_case("wiff2") => {
                files.lone_wiff2.push(name.to_string());
            }
            _ => {}
        }
    }
    // paired by stem ignoring case, like the worker does
    files.lone_wiff2.retain(|lone| {
        let stem = lone[..lone.len() - ".wiff2".len()].to_ascii_lowercase();
        !stems.contains(&stem)
    });
    files.missing_scans.sort_unstable();
    files.lone_wiff2.sort_unstable();
    files
}

// counts usable transition rows after the csv header
fn count_transition_rows(path: &Path) -> usize {
    let Ok(contents) = fs::read_to_string(path) else {
        return 0;
    };
    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .skip(1)
        .count()
}

// lists plot binaries in acquisition name order
fn plot_files(project: &Path) -> Result<Vec<PathBuf>, String> {
    let misc = project.join("misc");
    if !project.is_dir() {
        return Err("the selected dataset folder does not exist".to_string());
    }
    if !misc.is_dir() {
        return Err("run EchoMSkit before opening the visualizer".to_string());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(misc)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("plot_") && name.ends_with(".bin"))
        })
        .collect();
    files.sort_unstable();
    if files.is_empty() {
        return Err("no EchoMSkit plot data was found in misc".to_string());
    }
    Ok(files)
}

// strips plot_ and .bin from a plot filename, leaving the data file name
fn sample_id_from_plot(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("plot_"))
        .and_then(|name| name.strip_suffix(".bin"))
        .unwrap_or("sample")
        .to_string()
}

// skips windows for transitions that are not being rendered
fn skip_windows(reader: &mut BinReader<'_>) -> Result<(), String> {
    let window_count = reader.u16()?;
    for _ in 0..window_count {
        let _ = reader.string()?;
        reader.skip(8)?;
    }
    Ok(())
}

// skips a full transition block in the binary plot file
fn skip_transition(reader: &mut BinReader<'_>) -> Result<(), String> {
    let point_count = reader.u32()? as usize;
    reader.skip(
        point_count
            .checked_mul(8)
            .ok_or_else(|| "the visualizer data file is too large".to_string())?,
    )?;
    skip_windows(reader)
}

// reads the transition names stored in a plot binary
fn transition_names(path: &Path) -> Result<Vec<String>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut reader = BinReader::new(&bytes);
    let transition_count = reader.u16()?;
    let mut names = Vec::with_capacity(usize::from(transition_count));
    for _ in 0..transition_count {
        let name = reader.string()?;
        names.push(name);
        skip_transition(&mut reader)?;
    }
    Ok(names)
}

// converts raw 1-based worker indices into a renderable integration window
fn parse_window(
    points: &[VisualizerPoint],
    label: String,
    sample_id: Option<String>,
    raw_start: u32,
    raw_end: u32,
) -> VisualizerWindow {
    if points.is_empty() {
        return VisualizerWindow {
            label,
            sample_id,
            start_index: 0,
            end_index: 0,
            rt_start: 0.0,
            rt_end: 0.0,
            apex_rt: 0.0,
            height: 0.0,
            area: 0.0,
        };
    }
    let last = points.len().saturating_sub(1);
    let mut start = (raw_start.saturating_sub(1) as usize).min(last);
    let mut end = (raw_end.saturating_sub(1) as usize).min(last);
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    let rt_start = points.get(start).map_or(0.0, |point| point.rt);
    let rt_end = points.get(end).map_or(rt_start, |point| point.rt);
    let mut apex_rt = rt_start;
    let mut height = 0.0_f32;
    let mut area = 0.0_f32;
    for point in &points[start..=end] {
        if point.intensity >= height {
            height = point.intensity;
            apex_rt = point.rt;
        }
    }
    for pair in points[start..=end].windows(2) {
        area += (pair[0].intensity + pair[1].intensity) * (pair[1].rt - pair[0].rt) * 30.0;
    }
    VisualizerWindow {
        label,
        sample_id,
        start_index: start,
        end_index: end,
        rt_start,
        rt_end,
        apex_rt,
        height,
        area,
    }
}

fn parse_selected_transition(
    path: &Path,
    transition: &str,
    sample_ids: &SampleIds,
) -> Result<Option<VisualizerSample>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let mut reader = BinReader::new(&bytes);
    let transition_count = reader.u16()?;
    for _ in 0..transition_count {
        let name = reader.string()?;
        if name != transition {
            skip_transition(&mut reader)?;
            continue;
        }

        let point_count = reader.u32()? as usize;
        let mut points = Vec::with_capacity(point_count.min(bytes.len() / 8));
        for _ in 0..point_count {
            points.push(VisualizerPoint {
                rt: reader.f32()?,
                intensity: reader.f32()?,
            });
        }
        let sample_id = sample_id_from_plot(path);
        let window_count = reader.u16()?;
        let mut wells = Vec::with_capacity(usize::from(window_count));
        for _ in 0..window_count {
            let label = reader.string()?;
            let start = reader.u32()?;
            let end = reader.u32()?;
            let well_sample_id = sample_ids.get(&sample_id, &label);
            let well = parse_window(&points, label, well_sample_id, start, end);
            if well.end_index > well.start_index
                && well.rt_end > well.rt_start
                && well.height.is_finite()
                && well.height > 0.0
                && well.area.is_finite()
                && well.area > 0.0
            {
                wells.push(well);
            }
        }
        return Ok(Some(VisualizerSample {
            display_name: sample_display_name(&sample_id),
            sample_id,
            points,
            wells,
        }));
    }
    Ok(None)
}

// validates a dataset and summarizes its outputs
fn inspect_project(project: &Path) -> ProjectSummary {
    let mut issues = Vec::new();
    let mut transition_file = None;
    let mut transition_count = 0;
    let param_path = project.join("param.txt");
    let data_dir = project.join(DATA_DIR);
    let data = data_files(&data_dir);
    let worker = find_worker(project);

    if !param_path.is_file() {
        issues.push("param.txt was not found".to_string());
    } else {
        match fs::read_to_string(&param_path)
            .map_err(|error| error.to_string())
            .and_then(|contents| {
                contents
                    .parse::<toml::Table>()
                    .map_err(|error| error.to_string())
            }) {
            Ok(parameters) => {
                if let Some(value) = parameters
                    .get("transition_list")
                    .and_then(toml::Value::as_str)
                {
                    let path = project_file(project, value);
                    transition_file = Some(display_name(project, &path));
                    if path.is_file() {
                        transition_count = count_transition_rows(&path);
                    } else {
                        issues.push("the transition list was not found".to_string());
                    }
                } else {
                    issues.push("transition_list is missing from param.txt".to_string());
                }
                issues.extend(parameter_issues(&parameters));
            }
            Err(error) => issues.push(format!("param.txt could not be read: {error}")),
        }
    }

    if !data_dir.is_dir() {
        issues.push("wiff_dir was not found".to_string());
    } else {
        if !data.lone_wiff2.is_empty() {
            issues.push(format!(
                "{} has no matching .wiff, copy in the .wiff and .wiff.scan SCIEX OS writes next to it",
                data.lone_wiff2.join(", ")
            ));
        } else if data.wiff == 0 {
            issues.push("no .wiff files were found in wiff_dir".to_string());
        }
        if !data.missing_scans.is_empty() {
            issues.push(format!(
                "missing the matching .wiff.scan file: {}",
                data.missing_scans.join(", ")
            ));
        }
    }
    if worker.is_none() {
        issues.push("the EchoMSkit processing engine was not found".to_string());
    }

    let outputs = ProjectOutputs {
        acq_time: project.join(ACQ_TIME).is_file(),
        results_csv: preferred_results_file(project).is_file(),
        misc_data: project.join("misc").is_dir(),
    };

    ProjectSummary {
        name: project.file_name().map_or_else(
            || "project".to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
        path: project.to_string_lossy().into_owned(),
        data_count: data.wiff,
        transition_count,
        transition_file,
        worker_name: worker.and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        }),
        can_run: issues.is_empty(),
        setup: Vec::new(),
        issues,
        outputs,
    }
}

#[tauri::command]
// opens the slinghub page with the system browser
fn open_sling() -> Result<(), String> {
    const SLING_URL: &str = "https://sling.sg/";

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler").arg(SLING_URL);
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        command
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(SLING_URL);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(SLING_URL);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("the SLING website could not be opened: {error}"))
}

#[tauri::command]
// returns transition names for the selector
fn visualizer_list_transitions(project_path: String) -> Result<Vec<String>, String> {
    let project = PathBuf::from(project_path);
    let first = plot_files(&project)?
        .into_iter()
        .next()
        .ok_or_else(|| "no EchoMSkit plot data was found in misc".to_string())?;
    transition_names(&first)
}

#[tauri::command]
// reads one transition from every plot binary
fn visualizer_transition(
    project_path: String,
    transition: String,
) -> Result<VisualizerTransitionData, String> {
    if transition.trim().is_empty() {
        return Err("select a transition before rendering".to_string());
    }
    let project = PathBuf::from(project_path);
    let mut samples = Vec::new();
    let mut well_count = 0;
    let mut global_rt_min = f32::INFINITY;
    let mut global_rt_max = f32::NEG_INFINITY;
    let mut global_intensity_max = 0.0_f32;
    let mut transition_seen = false;
    let sample_ids = sample_ids(&project);

    for path in plot_files(&project)? {
        if let Some(sample) = parse_selected_transition(&path, &transition, &sample_ids)? {
            transition_seen = true;
            if !sample.wells.is_empty() {
                for point in &sample.points {
                    global_rt_min = global_rt_min.min(point.rt);
                    global_rt_max = global_rt_max.max(point.rt);
                    global_intensity_max = global_intensity_max.max(point.intensity);
                }
                well_count += sample.wells.len();
                samples.push(sample);
            }
        }
    }

    if samples.is_empty() {
        let message = if transition_seen {
            format!("transition {transition} has no nonblank sample plots")
        } else {
            format!("transition {transition} was not found in EchoMSkit plot data")
        };
        return Err(message);
    }
    if !global_rt_min.is_finite() || !global_rt_max.is_finite() {
        global_rt_min = 0.0;
        global_rt_max = 1.0;
    }

    Ok(VisualizerTransitionData {
        transition,
        sample_count: samples.len(),
        well_count,
        samples,
        global_rt_min,
        global_rt_max,
        global_intensity_max,
    })
}

#[tauri::command]
// lists saved acq_time.csv versions
fn visualizer_list_bounds_backups(
    app: AppHandle,
    project_path: String,
) -> Result<BackupList, String> {
    let project = PathBuf::from(&project_path);
    if !project.is_dir() {
        return Err("the selected dataset folder does not exist".to_string());
    }
    ensure_original_bounds(&project)?;
    let directory = project.join(BOUNDS_BACKUP_DIR);
    let backups: Vec<String> = if directory.is_dir() {
        fs::read_dir(&directory)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "csv")
            })
            .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
            .collect()
    } else {
        Vec::new()
    };
    let backups = order_backups(backups);
    let settings = read_settings(&app).unwrap_or_default();
    let last = settings
        .last_backups
        .get(&project_path)
        .cloned()
        .filter(|name| backups.contains(name));
    let labels = settings
        .backup_labels
        .get(&project_path)
        .map(|labels| {
            labels
                .iter()
                .filter(|(name, _)| backups.contains(name))
                .map(|(name, label)| (name.clone(), label.clone()))
                .collect()
        })
        .unwrap_or_default();
    Ok(BackupList {
        backups,
        last,
        labels,
    })
}

#[tauri::command]
// returns editable echomskit input files
fn data_editor_files(project_path: String) -> Result<Vec<DataEditorFile>, String> {
    Ok(data_editor_files_for_project(&PathBuf::from(project_path)))
}

#[tauri::command]
// reads one editable input file
fn data_editor_read(project_path: String, kind: String) -> Result<DataEditorContent, String> {
    let project = PathBuf::from(project_path);
    let path = data_editor_path(&project, &kind)?;
    read_data_editor_content_from_path(&project, &kind, &path)
}

#[tauri::command]
// previews one editable input backup
fn data_editor_read_backup(
    project_path: String,
    kind: String,
    backup: String,
) -> Result<DataEditorContent, String> {
    let project = PathBuf::from(project_path);
    let backups = list_data_editor_backups(&project, &kind)?;
    if !backups.contains(&backup) {
        return Err("that backup was not found".to_string());
    }
    let path = data_editor_backup_dir(&project, &kind)?.join(backup);
    read_data_editor_content_from_path(&project, &kind, &path)
}

#[tauri::command]
// saves one editable input file
fn data_editor_save(
    project_path: String,
    input: DataEditorSaveInput,
) -> Result<DataEditorContent, String> {
    let project = PathBuf::from(project_path);
    let path = data_editor_path(&project, &input.kind)?;
    let _ = ensure_data_editor_original_backup(&project, &input.kind)?;
    let _ = backup_data_editor_file(&project, &input.kind, &path)?;
    if let Some(text) = input.text {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&path, text).map_err(|error| error.to_string())?;
    } else {
        write_csv_table(&path, &input.headers, &input.rows)?;
    }
    read_data_editor_content_from_path(&project, &input.kind, &path)
}

#[tauri::command]
// restores an editable input backup over the live file
fn data_editor_restore_backup(
    project_path: String,
    kind: String,
    backup: String,
) -> Result<DataEditorContent, String> {
    let project = PathBuf::from(project_path);
    let backups = list_data_editor_backups(&project, &kind)?;
    if !backups.contains(&backup) {
        return Err("that backup was not found".to_string());
    }
    let live = data_editor_path(&project, &kind)?;
    let source = data_editor_backup_dir(&project, &kind)?.join(backup);
    let _ = backup_data_editor_file(&project, &kind, &live)?;
    if let Some(parent) = live.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::copy(source, &live).map_err(|error| error.to_string())?;
    read_data_editor_content_from_path(&project, &kind, &live)
}

#[tauri::command]
// renames an editable input backup
fn data_editor_rename_backup(
    project_path: String,
    kind: String,
    backup: String,
    name: String,
) -> Result<String, String> {
    let project = PathBuf::from(project_path);
    if backup == data_editor_original_backup_name(&project, &kind)? {
        return Err("original cannot be renamed".to_string());
    }
    let backups = list_data_editor_backups(&project, &kind)?;
    if !backups.contains(&backup) {
        return Err("that backup was not found".to_string());
    }
    let renamed = safe_data_editor_backup_name(&name, &backup)?;
    let directory = data_editor_backup_dir(&project, &kind)?;
    let target = directory.join(&renamed);
    if target.exists() {
        return Err("a backup with that name already exists".to_string());
    }
    fs::rename(directory.join(backup), target).map_err(|error| error.to_string())?;
    Ok(renamed)
}

#[tauri::command]
// deletes an editable input backup
fn data_editor_delete_backup(
    project_path: String,
    kind: String,
    backup: String,
) -> Result<DataEditorContent, String> {
    let project = PathBuf::from(project_path);
    if backup == data_editor_original_backup_name(&project, &kind)? {
        return Err("original cannot be deleted".to_string());
    }
    let backups = list_data_editor_backups(&project, &kind)?;
    if !backups.contains(&backup) {
        return Err("that backup was not found".to_string());
    }
    fs::remove_file(data_editor_backup_dir(&project, &kind)?.join(backup))
        .map_err(|error| error.to_string())?;
    let live = data_editor_path(&project, &kind)?;
    read_data_editor_content_from_path(&project, &kind, &live)
}

#[tauri::command]
// renames the visible backup label
fn visualizer_rename_bounds_backup(
    app: AppHandle,
    project_path: String,
    name: String,
    label: String,
) -> Result<(), String> {
    let project = PathBuf::from(&project_path);
    if !project.is_dir() {
        return Err("the selected dataset folder does not exist".to_string());
    }
    let name = safe_backup_name(&name)?.to_string();
    if name.eq_ignore_ascii_case(ORIGINAL_BOUNDS_BACKUP) {
        return Err("Original acq_time.csv cannot be renamed".to_string());
    }
    if !project.join(BOUNDS_BACKUP_DIR).join(&name).is_file() {
        return Err("that backup version no longer exists".to_string());
    }
    let label = safe_backup_label(&label)?;
    let mut settings = read_settings(&app).unwrap_or_default();
    settings
        .backup_labels
        .entry(project_path)
        .or_default()
        .insert(name, label);
    write_settings(&app, &settings)
}

#[tauri::command]
// removes a saved backup file
fn visualizer_delete_bounds_backup(
    app: AppHandle,
    project_path: String,
    name: String,
) -> Result<(), String> {
    let project = PathBuf::from(&project_path);
    if !project.is_dir() {
        return Err("the selected dataset folder does not exist".to_string());
    }
    let name = safe_backup_name(&name)?.to_string();
    if name.eq_ignore_ascii_case(ORIGINAL_BOUNDS_BACKUP) {
        return Err("Original acq_time.csv cannot be deleted".to_string());
    }
    let source = project.join(BOUNDS_BACKUP_DIR).join(&name);
    if !source.is_file() {
        return Err("that backup version no longer exists".to_string());
    }
    fs::remove_file(source).map_err(|error| error.to_string())?;
    let mut settings = read_settings(&app).unwrap_or_default();
    if settings
        .last_backups
        .get(&project_path)
        .is_some_and(|last| last == &name)
    {
        settings.last_backups.remove(&project_path);
    }
    let remove_project_labels = if let Some(labels) = settings.backup_labels.get_mut(&project_path)
    {
        labels.remove(&name);
        labels.is_empty()
    } else {
        false
    };
    if remove_project_labels {
        settings.backup_labels.remove(&project_path);
    }
    write_settings(&app, &settings)
}

// recalculates areas after acq_time.csv changes, skipping detection so the edits stand
fn rerun_after_bounds_change(app: AppHandle, project: PathBuf) -> Result<ProjectSummary, String> {
    run_worker(app, project, &[AREA_STEP])
}

#[tauri::command]
// restores a backup and refreshes plots
async fn visualizer_restore_bounds_backup(
    app: AppHandle,
    state: State<'_, RunState>,
    project_path: String,
    name: String,
) -> Result<SaveBoundsResult, String> {
    let project = PathBuf::from(&project_path);
    if !project.is_dir() {
        return Err("the selected dataset folder does not exist".to_string());
    }
    let name = safe_backup_name(&name)?.to_string();
    let run = state.start()?;
    emit_status(&app, "running");
    let restore_name = name.clone();
    let worker_app = app.clone();
    let worker_project = project.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let source = worker_project.join(BOUNDS_BACKUP_DIR).join(&restore_name);
        if !source.is_file() {
            return Err("that backup version no longer exists".to_string());
        }
        fs::copy(&source, worker_project.join(ACQ_TIME)).map_err(|error| error.to_string())?;
        rerun_after_bounds_change(worker_app, worker_project)
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result)
    .map(|project| SaveBoundsResult {
        written: 0,
        backup: Some(name.clone()),
        project,
    });

    drop(run);
    emit_status(&app, if result.is_ok() { "complete" } else { "failed" });
    if result.is_ok() {
        let mut settings = read_settings(&app).unwrap_or_default();
        settings.last_backups.insert(project_path, name);
        let _ = write_settings(&app, &settings);
    }
    result
}

#[tauri::command]
// saves edited bounds and refreshes plots
async fn visualizer_save_bounds(
    app: AppHandle,
    state: State<'_, RunState>,
    project_path: String,
    edits: Vec<BoundEdit>,
    backup_name: String,
) -> Result<SaveBoundsResult, String> {
    if edits.is_empty() {
        return Err("no integration bounds have changed".to_string());
    }
    let project = PathBuf::from(&project_path);
    if !project.is_dir() {
        return Err("the selected dataset folder does not exist".to_string());
    }
    let name = writable_bounds_backup_name(&backup_name)?.to_string();
    let run = state.start()?;
    emit_status(&app, "running");
    let worker_app = app.clone();
    let worker_project = project.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        ensure_original_bounds(&worker_project)?;
        let written = save_bounds_to_acq_time(&worker_project, &edits)?;
        let backup = backup_bounds(&worker_project, &name)?;
        let summary = rerun_after_bounds_change(worker_app, worker_project)?;
        Ok(SaveBoundsResult {
            written,
            backup,
            project: summary,
        })
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);

    drop(run);
    emit_status(&app, if result.is_ok() { "complete" } else { "failed" });
    if let Ok(result) = &result
        && let Some(backup) = &result.backup
    {
        let mut settings = read_settings(&app).unwrap_or_default();
        settings.last_backups.insert(project_path, backup.clone());
        let _ = write_settings(&app, &settings);
    }
    result
}

#[tauri::command]
// loads remembered project state on startup
fn load_startup_state(app: AppHandle) -> StartupState {
    let settings = read_settings(&app);
    let remembered = settings.as_ref().and_then(|settings| {
        (!settings.last_project.is_empty()).then(|| PathBuf::from(&settings.last_project))
    });
    let project = remembered
        .as_deref()
        .filter(|path| path.is_dir())
        .map(inspect_project);
    StartupState {
        remembered_path: remembered
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        needs_reselection: remembered.is_some() && project.is_none(),
        project,
        theme: settings.and_then(|settings| settings.theme),
    }
}

#[tauri::command]
// selects and remembers a project folder
fn select_project(app: AppHandle, path: String) -> Result<ProjectSummary, String> {
    let project = PathBuf::from(path);
    if !project.is_dir() {
        return Err("the selected project folder does not exist".to_string());
    }
    save_project(&app, &project)?;
    Ok(scan_project(&project))
}

#[tauri::command]
// refreshes project state without changing settings
fn refresh_project(path: String) -> Result<ProjectSummary, String> {
    let project = PathBuf::from(path);
    if !project.is_dir() {
        return Err("the selected project folder does not exist".to_string());
    }
    Ok(scan_project(&project))
}

// sets up a raw export before inspecting it, so a fresh folder is ready to run
fn scan_project(project: &Path) -> ProjectSummary {
    let setup = project_setup::prepare(project);
    ProjectSummary {
        setup,
        ..inspect_project(project)
    }
}

#[tauri::command]
// saves the preferred theme
fn set_theme(app: AppHandle, theme: String) -> Result<(), String> {
    if theme != "light" && theme != "dark" {
        return Err("the interface theme must be light or dark".to_string());
    }
    let mut settings = read_settings(&app).unwrap_or_default();
    settings.theme = Some(theme);
    write_settings(&app, &settings)
}

// streams worker output back to the ui
fn forward_output<R: Read>(
    reader: R,
    app: &AppHandle,
    stream: &str,
    prefix: &str,
) -> Result<(), String> {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        match reader.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' || byte[0] == b'\r' => {
                if !line.is_empty() {
                    emit_output(app, stream, prefix, &line);
                    line.clear();
                }
            }
            Ok(_) => line.push(byte[0]),
            Err(error) => return Err(error.to_string()),
        }
    }
    if !line.is_empty() {
        emit_output(app, stream, prefix, &line);
    }
    Ok(())
}

// emits one worker log line
fn emit_output(app: &AppHandle, stream: &str, prefix: &str, line: &[u8]) {
    let text = String::from_utf8_lossy(line).trim().to_string();
    if text.is_empty() {
        return;
    }
    let _ = app.emit(
        "worker-output",
        WorkerOutput {
            stream: stream.to_string(),
            line: format!("{prefix}: {text}"),
        },
    );
}

// emits worker running state
fn emit_status(app: &AppHandle, status: &str) {
    let _ = app.emit(
        "run-state",
        RunStatus {
            status: status.to_string(),
        },
    );
}

// runs one EchoMSkit command step
fn run_worker_command(
    app: &AppHandle,
    project: &Path,
    worker: &Path,
    arg: &str,
    label: &str,
) -> Result<(), String> {
    let _ = app.emit(
        "worker-output",
        WorkerOutput {
            stream: "output".to_string(),
            line: format!("Starting {label}."),
        },
    );

    let mut command = Command::new(worker);
    command
        .arg(arg)
        .current_dir(project)
        .env("ECHOMSKIT_PROJECT_DIR", project)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }

    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "the processing output stream was unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "the processing error stream was unavailable".to_string())?;

    let stderr_app = app.clone();
    let stderr_label = label.to_string();
    let stderr_thread =
        std::thread::spawn(move || forward_output(stderr, &stderr_app, "error", &stderr_label));
    forward_output(stdout, app, "output", label)?;
    let status = child.wait().map_err(|error| error.to_string())?;
    stderr_thread
        .join()
        .map_err(|_| "the processing error reader stopped unexpectedly".to_string())??;

    if !status.success() {
        return Err(format!(
            "{label} exited with code {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

// runs worker steps and labels the results for the gui
fn run_worker(
    app: AppHandle,
    project: PathBuf,
    steps: &[(&str, &str)],
) -> Result<ProjectSummary, String> {
    let summary = inspect_project(&project);
    if !summary.can_run {
        return Err(summary.issues.join("; "));
    }
    let worker = find_worker(&project)
        .ok_or_else(|| "the EchoMSkit processing engine was not found".to_string())?;
    let had_long_csv = project.join("long.csv").is_file();

    for (arg, label) in steps {
        run_worker_command(&app, &project, &worker, arg, label)?;
    }
    write_labeled_results(&project)?;
    cleanup_generated_long_csv(&project, had_long_csv)?;
    Ok(inspect_project(&project))
}

#[tauri::command]
// runs EchoMSkit while preventing duplicate runs
async fn run_all(
    app: AppHandle,
    state: State<'_, RunState>,
    path: String,
) -> Result<ProjectSummary, String> {
    let project = PathBuf::from(path);
    if !project.is_dir() {
        return Err("the selected project folder does not exist".to_string());
    }
    let run = state.start()?;
    emit_status(&app, "running");
    let worker_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        run_worker(worker_app, project, &[DETECT_STEP, AREA_STEP])
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result);

    drop(run);
    emit_status(&app, if result.is_ok() { "complete" } else { "failed" });
    result
}

fn main() {
    tauri::Builder::default()
        .manage(RunState(AtomicBool::new(false)))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_startup_state,
            select_project,
            refresh_project,
            set_theme,
            open_sling,
            visualizer_list_transitions,
            visualizer_transition,
            visualizer_list_bounds_backups,
            data_editor_files,
            data_editor_read,
            data_editor_read_backup,
            data_editor_save,
            data_editor_restore_backup,
            data_editor_rename_backup,
            data_editor_delete_backup,
            visualizer_rename_bounds_backup,
            visualizer_delete_bounds_backup,
            visualizer_restore_bounds_backup,
            visualizer_save_bounds,
            run_all
        ])
        .run(tauri::generate_context!())
        .expect("error while running EchoMSkit GUI");
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestProject(PathBuf);

    impl TestProject {
        fn new(label: &str) -> Self {
            static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "echomskit_{label}_{}_{}_{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn csv_editor_round_trips_quoted_multiline_cells_and_crlf() {
        let project = TestProject::new("csv_roundtrip");
        let path = project.0.join("samples.csv");
        let headers = vec!["File".into(), "Index".into(), "Sample_ID".into()];
        let rows = vec![
            vec![
                "run".into(),
                "1".into(),
                "first, \"quoted\"\r\nsecond".into(),
            ],
            vec!["run".into(), "2".into(), "".into()],
        ];
        write_csv_table(&path, &headers, &rows).unwrap();
        assert_eq!(read_csv_table(&path, &[]).unwrap(), (headers, rows));
    }

    #[test]
    fn csv_editor_preserves_row_widths_and_cell_whitespace() {
        let project = TestProject::new("csv_rows");
        let path = project.0.join("samples.csv");
        fs::write(&path, "\u{feff}A,B\r\n\r\n short \r\n1,2,3\r\n,\r\n").unwrap();
        let (headers, rows) = read_csv_table(&path, &[]).unwrap();
        assert_eq!(headers, ["A", "B"]);
        assert_eq!(rows, [vec![" short "], vec!["1", "2", "3"], vec!["", ""]]);
        write_csv_table(&path, &headers, &rows).unwrap();
        assert_eq!(read_csv_table(&path, &[]).unwrap(), (headers, rows));
    }

    #[test]
    fn labeled_results_match_file_rows_before_shared_rows() {
        let project = TestProject::new("labeled_results");
        fs::write(
            project.0.join(DEFAULT_SAMPLE_LIST),
            "File,Index,Sample_ID\nrun.wiff,1,SN0323\n,1,shared\n,2,\"sample \"\"quoted\"\"\r\nsecond line\"\nRUN,3,stem match\n",
        )
        .unwrap();
        fs::write(
            project.0.join("long.csv"),
            "sample,ID,well,window start,window end,RT at apex,area,height\n\
             run.wiff,Cer,1 B2:Marker,0.1,0.6,0.223,13935.9,3070.1\n\
             other.wiff,Cer,1 B2,0.1,0.6,0.223,1.0,1.0\n\
             run.wiff,\"Cer\ntransition\",2 B2,0.3,0.8,0.5,2.0,2.0\n\
             run.wiff,Cer,3 B2,0.5,1.0,0.7,3.0,3.0\n\
             run.wiff,Cer,4 B2,0.7,1.2,0.9,4.0,4.0\n",
        )
        .unwrap();
        write_labeled_results(&project.0).unwrap();
        let (headers, rows) = read_csv_table(&project.0.join(RESULTS), &[]).unwrap();
        assert_eq!(
            headers,
            [
                "sample",
                "ID",
                "well",
                "Sample_ID",
                "window start",
                "window end",
                "RT at apex",
                "area",
                "height"
            ]
        );
        let ids = rows.iter().map(|row| row[3].as_str()).collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "SN0323",
                "shared",
                "sample \"quoted\"\r\nsecond line",
                "stem match",
                ""
            ]
        );
        assert_eq!(rows[2][1], "Cer\ntransition");
    }

    #[test]
    fn run_state_rejects_duplicates_and_releases_after_early_error() {
        let state = RunState(AtomicBool::new(false));
        let run = state.start().unwrap();
        assert!(state.start().is_err());
        drop(run);
        let failing_operation = || -> Result<(), String> {
            let _run = state.start()?;
            safe_backup_name("../invalid.csv")?;
            Ok(())
        };
        assert!(failing_operation().is_err());
        let _retry = state.start().unwrap();
    }

    #[test]
    fn project_inspection_checks_param_values_like_the_worker() {
        let project = TestProject::new("param_values");
        let issue = |param: &str| {
            fs::write(project.0.join("param.txt"), param).unwrap();
            inspect_project(&project.0)
                .issues
                .into_iter()
                .filter(|issue| {
                    issue.contains("sample_duration") || issue.contains("mass_tolerance")
                })
                .collect::<Vec<_>>()
        };
        for valid in [
            "sample_duration = 30\n",
            "sample_duration = 15.5\nmass_tolerance = 0.06\n",
            "sample_duration = 30\nmass_tolerance = 1\n",
        ] {
            assert!(issue(valid).is_empty(), "{valid}");
        }
        for invalid in [
            "",
            "sample_duration = 0\n",
            "sample_duration = -5\n",
            "sample_duration = \"30\"\n",
            "sample_duration = nan\n",
            "sample_duration = 30\nmass_tolerance = 0\n",
            "sample_duration = 30\nmass_tolerance = inf\n",
        ] {
            assert_eq!(issue(invalid).len(), 1, "{invalid}");
        }
    }

    #[test]
    fn project_inspection_reports_wiff_files_without_scan_data() {
        let project = TestProject::new("missing_scans");
        let data = project.0.join(DATA_DIR);
        fs::create_dir(&data).unwrap();
        fs::write(data.join("a.wiff"), []).unwrap();
        fs::write(data.join("a.wiff.scan"), []).unwrap();
        fs::write(data.join("b.WIFF"), []).unwrap();
        fs::write(data.join("B.wiff2"), []).unwrap();
        fs::write(data.join("a.wiff2"), []).unwrap();
        let files = data_files(&data);
        assert_eq!(
            (files.wiff, files.missing_scans),
            (2, vec!["b.WIFF".to_string()])
        );
        assert!(files.lone_wiff2.is_empty());
        let issues = inspect_project(&project.0).issues;
        assert!(issues.iter().any(|issue| issue.contains("b.WIFF")));

        fs::write(data.join("c.wiff2"), []).unwrap();
        assert_eq!(data_files(&data).lone_wiff2, ["c.wiff2"]);
        let issues = inspect_project(&project.0).issues;
        assert!(issues.iter().any(|issue| issue.contains("c.wiff2")));
    }

    fn plot_fixture() -> Vec<u8> {
        let mut bytes = 1_u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"Cer\0");
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        for (rt, intensity) in [(1.0_f32, 10.0_f32), (1.5, 30.0), (2.0, 10.0)] {
            bytes.extend_from_slice(&rt.to_le_bytes());
            bytes.extend_from_slice(&intensity.to_le_bytes());
        }
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(b"1 B2\0");
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes
    }

    #[test]
    fn plot_binary_parses_integration_and_rejects_every_truncated_prefix() {
        let project = TestProject::new("plot_binary");
        let path = project.0.join("plot_run.wiff.bin");
        let bytes = plot_fixture();
        let ids = SampleIds::default();
        for end in 0..bytes.len() {
            fs::write(&path, &bytes[..end]).unwrap();
            assert!(transition_names(&path).is_err(), "accepted {end} bytes");
            assert!(
                parse_selected_transition(&path, "Cer", &ids).is_err(),
                "accepted {end} bytes"
            );
        }
        fs::write(&path, bytes).unwrap();
        assert_eq!(transition_names(&path).unwrap(), ["Cer"]);
        let sample = parse_selected_transition(&path, "Cer", &ids)
            .unwrap()
            .unwrap();
        assert_eq!(sample.sample_id, "run.wiff");
        assert_eq!(sample.display_name, "run");
        assert_eq!(sample.points.len(), 3);
        assert_eq!(sample.wells.len(), 1);
        let well = &sample.wells[0];
        assert_eq!(well.label, "1 B2");
        assert_eq!((well.start_index, well.end_index), (0, 2));
        assert_eq!((well.rt_start, well.apex_rt, well.rt_end), (1.0, 1.5, 2.0));
        assert_eq!((well.height, well.area), (30.0, 1200.0));
        assert!(
            parse_selected_transition(&path, "missing", &ids)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn huge_point_counts_fail_instead_of_allocating() {
        let project = TestProject::new("plot_huge");
        let path = project.0.join("plot_run.wiff.bin");
        let mut bytes = 1_u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"Cer\0");
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        assert!(transition_names(&path).is_err());
        assert!(parse_selected_transition(&path, "Cer", &SampleIds::default()).is_err());
    }

    #[test]
    fn saved_bounds_keep_the_duration_tag_and_other_windows() {
        let project = TestProject::new("save_bounds");
        let path = project.0.join(ACQ_TIME);
        fs::write(
            &path,
            "# sample_duration = 30\nrun.wiff,,\n1 B2:Marker,0.594,1.094\n2 B2,1.437,1.937\nnext.wiff,,\n1 B2,0.500,1.000\n",
        )
        .unwrap();
        let edit = |sample_id: &str, well_label: &str, rt_start, rt_end| BoundEdit {
            sample_id: sample_id.into(),
            well_label: well_label.into(),
            rt_start,
            rt_end,
        };
        let written = save_bounds_to_acq_time(
            &project.0,
            &[
                edit("run.wiff", "2 B2", 1.9, 1.4),
                edit("next.wiff", "1 B2", 0.55, 1.05),
            ],
        )
        .unwrap();
        assert_eq!(written, 2);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# sample_duration = 30\nrun.wiff,,\n1 B2:Marker,0.594,1.094\n2 B2,1.400,1.900\nnext.wiff,,\n1 B2,0.550,1.050\n"
        );

        let before = fs::read(&path).unwrap();
        for bad in [
            edit("run.wiff", "9 B2", 0.1, 0.2),
            edit("missing.wiff", "1 B2", 0.1, 0.2),
            edit("run.wiff", "1 B2:Marker", f32::NAN, 0.2),
        ] {
            assert!(save_bounds_to_acq_time(&project.0, &[bad]).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }

    #[test]
    fn editor_backups_preserve_every_snapshot() {
        let project = TestProject::new("editor_snapshots");
        let live = project.0.join("param.txt");
        let mut snapshots = Vec::new();
        for content in ["first", "second", "third", "fourth"] {
            fs::write(&live, content).unwrap();
            let name = backup_data_editor_file(&project.0, "param", &live)
                .unwrap()
                .unwrap();
            snapshots.push((name, content));
        }
        for (name, content) in snapshots {
            assert_eq!(
                fs::read_to_string(project.0.join("data_file_backups/param").join(name)).unwrap(),
                content
            );
        }
    }

    #[test]
    fn bounds_backup_cannot_overwrite_original() {
        let project = TestProject::new("protected_bounds");
        let live = project.0.join(ACQ_TIME);
        fs::write(&live, "original bounds").unwrap();
        ensure_original_bounds(&project.0).unwrap();
        fs::write(&live, "edited bounds").unwrap();
        assert!(backup_bounds(&project.0, ORIGINAL_BOUNDS_BACKUP).is_err());
        assert!(backup_bounds(&project.0, "ACQ_TIME_ORIGINAL.csv").is_err());
        assert!(backup_bounds(&project.0, "edit.rftime").is_err());
        assert_eq!(
            fs::read_to_string(
                project
                    .0
                    .join(BOUNDS_BACKUP_DIR)
                    .join(ORIGINAL_BOUNDS_BACKUP)
            )
            .unwrap(),
            "original bounds"
        );
    }

    #[test]
    fn editor_reports_unreadable_text_instead_of_empty_content() {
        let project = TestProject::new("invalid_param_encoding");
        fs::write(project.0.join("param.txt"), [0xff, 0xfe]).unwrap();
        assert!(
            data_editor_read(project.0.to_string_lossy().into_owned(), "param".into()).is_err()
        );
    }

    #[test]
    fn missing_sample_list_opens_with_the_detected_windows() {
        let project = TestProject::new("sample_list_template");
        let read = || {
            data_editor_read(
                project.0.to_string_lossy().into_owned(),
                "sampleList".into(),
            )
            .unwrap()
        };
        let empty = read();
        assert!(empty.rows.is_empty());
        assert!(empty.note.is_some());

        fs::write(
            project.0.join(ACQ_TIME),
            "# sample_duration = 30\nrun.wiff,,\n1 B2:Marker,0.594,1.094\n2 B3,1.437,1.937\nother.wiff,,\n1,0.5,1.0\n",
        )
        .unwrap();
        let template = read();
        assert_eq!(template.headers, ["File", "Index", "Well", "Sample_ID"]);
        assert_eq!(
            template.rows,
            [
                ["run.wiff", "1", "B2:Marker", ""],
                ["run.wiff", "2", "B3", ""],
                ["other.wiff", "1", "", ""],
            ]
        );

        // saved as is, the template gives every window its id once filled in
        let mut rows = template.rows;
        rows[1][3] = "SN0323".into();
        write_csv_table(
            &project.0.join(DEFAULT_SAMPLE_LIST),
            &template.headers,
            &rows,
        )
        .unwrap();
        assert_eq!(
            sample_ids(&project.0).get("run.wiff", "2 B3").as_deref(),
            Some("SN0323")
        );
        assert!(read().note.is_none());
    }

    #[test]
    fn editor_finds_transition_list_from_param_or_folder() {
        let project = TestProject::new("transition_lookup");
        assert_eq!(
            transition_table(&project.0),
            project.0.join(DEFAULT_TRANSITIONS)
        );
        fs::write(project.0.join("MRMHR transitions.csv"), "").unwrap();
        assert_eq!(
            transition_table(&project.0),
            project.0.join("MRMHR transitions.csv")
        );
        fs::write(
            project.0.join("param.txt"),
            "transition_list = 'lists/assay.csv'\n",
        )
        .unwrap();
        assert_eq!(
            transition_table(&project.0),
            project.0.join("lists/assay.csv")
        );
    }

    #[test]
    fn worker_name_uses_platform_extension() {
        if cfg!(windows) {
            assert_eq!(worker_name("EchoMSkit"), "EchoMSkit.exe");
        } else {
            assert_eq!(worker_name("EchoMSkit"), "EchoMSkit");
        }
    }

    #[test]
    fn transition_counter_ignores_header_and_comments() {
        let project = TestProject::new("transition_count");
        let path = project.0.join("transitions.csv");
        fs::write(&path, "# note\nName,Q1,Q3\nA,1,2\n\nB,3,\n").unwrap();
        assert_eq!(count_transition_rows(&path), 2);
    }

    #[test]
    fn window_labels_and_names_read_cleanly() {
        assert_eq!(window_index("12 B2:Marker"), Some(12));
        assert_eq!(window_index("7"), Some(7));
        assert_eq!(window_index("B2"), None);
        assert_eq!(sample_display_name("run.WIFF"), "run");
        assert_eq!(sample_display_name("run.wiff2"), "run.wiff2");
    }

    #[test]
    fn cleanup_generated_long_csv_only_removes_new_worker_output() {
        let project = TestProject::new("long_cleanup");
        fs::write(project.0.join("long.csv"), "sample,ID,well\n").unwrap();
        fs::write(project.0.join(RESULTS), "sample,ID,well,Sample_ID\n").unwrap();

        cleanup_generated_long_csv(&project.0, false).unwrap();
        assert!(!project.0.join("long.csv").exists());

        fs::write(project.0.join("long.csv"), "sample,ID,well\n").unwrap();
        cleanup_generated_long_csv(&project.0, true).unwrap();
        assert!(project.0.join("long.csv").exists());
    }

    #[test]
    fn plot_files_sort_by_acquisition_name() {
        let project = TestProject::new("plot_sort");
        let misc = project.0.join("misc");
        fs::create_dir_all(&misc).unwrap();
        for name in ["plot_b.wiff.bin", "plot_a.wiff.bin", "notes.txt"] {
            fs::write(misc.join(name), []).unwrap();
        }
        let names: Vec<String> = plot_files(&project.0)
            .unwrap()
            .into_iter()
            .map(|path| sample_id_from_plot(&path))
            .collect();
        assert_eq!(names, vec!["a.wiff", "b.wiff"]);
    }
}
