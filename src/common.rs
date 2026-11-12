use crate::wiff::scan_path;
use std::error::Error;
use std::path::PathBuf;

pub const DATA_DIR: &str = "wiff_dir";
pub const ACQ_TIME: &str = "acq_time.csv";
const DURATION_TAG: &str = "sample_duration";

pub struct Param {
    pub t_list: PathBuf,
    // seconds of signal per ejection, windows add a scan cycle on each side
    pub sample_duration: f32,
    // ± Da around each target, overrides the transition list xic widths when set
    pub mass_tolerance: Option<f64>,
}

fn number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|value| value as f64))
}

pub fn read_param() -> Result<Param, Box<dyn Error>> {
    let param = std::fs::read_to_string("param.txt")?;
    let value = param.parse::<toml::Table>()?;
    let transition_list = value
        .get("transition_list")
        .and_then(toml::Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .ok_or("transition_list must be a nonempty path in param.txt")?;
    let sample_duration = value
        .get(DURATION_TAG)
        .and_then(number)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .ok_or("sample_duration must be a positive number of seconds in param.txt")?;
    let mass_tolerance = value
        .get("mass_tolerance")
        .map(|tolerance| {
            number(tolerance)
                .filter(|tolerance| tolerance.is_finite() && *tolerance > 0.0)
                .ok_or("mass_tolerance must be a positive number of Da in param.txt")
        })
        .transpose()?;
    Ok(Param {
        t_list: PathBuf::from(transition_list),
        sample_duration: sample_duration as f32,
        mass_tolerance,
    })
}

/* .wiff files in the data folder, sorted by name. a .wiff2 on its own is an
error since its scans are encrypted, SCIEX OS writes the readable .wiff and
.wiff.scan pair next to it */
pub fn data_files() -> Result<Vec<String>, Box<dyn Error>> {
    let entries = std::fs::read_dir(DATA_DIR)
        .map_err(|error| format!("{DATA_DIR} could not be opened: {error}"))?;
    let mut files = Vec::new();
    let mut wiff2 = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("wiff") && path.is_file() => {
                if !scan_path(&path).is_file() {
                    println!(
                        "{name:?} has no .wiff.scan next to it, its areas cannot be calculated"
                    );
                }
                files.push(name.to_string());
            }
            Some(extension) if extension.eq_ignore_ascii_case("wiff2") => {
                wiff2.push(name.to_string());
            }
            _ => {}
        }
    }
    // paired by stem ignoring case, extensions are ascii so the slices are safe
    wiff2.retain(|lone| {
        let stem = &lone[..lone.len() - ".wiff2".len()];
        !files
            .iter()
            .any(|file| file[..file.len() - ".wiff".len()].eq_ignore_ascii_case(stem))
    });
    if !wiff2.is_empty() {
        wiff2.sort();
        return Err(format!(
            "{} has no matching .wiff, EchoMSkit needs the .wiff and .wiff.scan that SCIEX OS writes next to each .wiff2",
            wiff2.join(", ")
        )
        .into());
    }
    if files.is_empty() {
        return Err(format!("{DATA_DIR} contains no .wiff files").into());
    }
    files.sort();
    Ok(files)
}

pub struct Acq {
    pub file: String,
    pub windows: Vec<(String, f32, f32)>,
}

pub fn duration_tag(seconds: f32) -> String {
    format!("# {DURATION_TAG} = {seconds}")
}

/* acq_time.csv is one "file,," row per data file followed by "label,start,end"
rows in minutes. the first line records the sample duration the windows were
detected with */
pub fn read_acq_time() -> Result<(Option<f32>, Vec<Acq>), Box<dyn Error>> {
    let text = std::fs::read_to_string(ACQ_TIME)?;
    let duration = text
        .lines()
        .next()
        .and_then(|line| line.strip_prefix('#'))
        .and_then(|tag| tag.split_once('='))
        .filter(|(key, _)| key.trim() == DURATION_TAG)
        .and_then(|(_, value)| value.trim().parse().ok());

    let rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .comment(Some(b'#'))
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let mut acq_time: Vec<Acq> = Vec::new();
    for row in rdr.into_records() {
        let row = row?;
        let line = row.position().map_or(0, |position| position.line());
        if row.len() < 3 || row[0].is_empty() {
            return Err(
                format!("{ACQ_TIME} line {line} requires a label and three columns").into(),
            );
        }
        if row[1].is_empty() && row[2].is_empty() {
            acq_time.push(Acq {
                file: row[0].to_string(),
                windows: Vec::new(),
            });
            continue;
        }
        let sample = acq_time
            .last_mut()
            .ok_or_else(|| format!("{ACQ_TIME} line {line} has a window before any file row"))?;
        let beg: f32 = row[1]
            .parse()
            .map_err(|err| format!("Invalid start time in {ACQ_TIME} line {line}: {err}"))?;
        let end: f32 = row[2]
            .parse()
            .map_err(|err| format!("Invalid end time in {ACQ_TIME} line {line}: {err}"))?;
        if !beg.is_finite() || !end.is_finite() || end < beg {
            return Err(format!("Invalid window in {ACQ_TIME} line {line}").into());
        }
        sample.windows.push((row[0].to_string(), beg, end));
    }
    if acq_time.is_empty() {
        return Err(format!("{ACQ_TIME} contains no data files").into());
    }
    Ok((duration, acq_time))
}
