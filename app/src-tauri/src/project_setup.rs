use crate::{DATA_DIR, DEFAULT_TRANSITIONS, find_named_csv, project_file, project_parameters};
use calamine::{Data, Range, Reader, open_workbook_auto};
use std::fs;
use std::path::{Path, PathBuf};

const SCIEX_SUFFIXES: [&str; 3] = [".wiff", ".wiff.scan", ".wiff2"];
const DEFAULT_DURATION: f64 = 30.0;

/* turns a raw SCIEX export into a project: acquisitions move into wiff_dir, the
transition workbook gets a csv copy and a missing param.txt is written with
defaults read off the folder. nothing existing is overwritten. returns what was
done, for the activity log */
pub fn prepare(project: &Path) -> Vec<String> {
    let mut notes = Vec::new();
    move_acquisitions(project, &mut notes);
    let transitions = transition_csv(project, &mut notes);
    if !project.join("param.txt").exists() {
        write_param(project, transitions.as_deref(), &mut notes);
    }
    notes
}

fn root_files(project: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = fs::read_dir(project) else {
        return Vec::new();
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_string();
            Some((path, name))
        })
        .collect::<Vec<_>>();
    files.sort_unstable_by(|left, right| left.1.cmp(&right.1));
    files
}

fn move_acquisitions(project: &Path, notes: &mut Vec<String>) {
    let data_dir = project.join(DATA_DIR);
    let mut moved = 0;
    for (path, name) in root_files(project) {
        let lower = name.to_ascii_lowercase();
        if !SCIEX_SUFFIXES.iter().any(|suffix| lower.ends_with(suffix)) {
            continue;
        }
        let target = data_dir.join(&name);
        if target.exists() {
            notes.push(format!("Left {name} in place, {DATA_DIR} already has one."));
            continue;
        }
        if !data_dir.is_dir() {
            if let Err(error) = fs::create_dir(&data_dir) {
                notes.push(format!("{DATA_DIR} could not be created: {error}."));
                return;
            }
            notes.push(format!("Created {DATA_DIR}."));
        }
        match fs::rename(&path, &target) {
            Ok(()) => moved += 1,
            Err(error) => notes.push(format!(
                "{name} could not be moved into {DATA_DIR}: {error}."
            )),
        }
    }
    if moved > 0 {
        notes.push(format!("Moved {moved} SCIEX file(s) into {DATA_DIR}."));
    }
}

/* the transition csv the project should use, as written in param.txt. an
existing csv wins, otherwise the transition workbook is converted next to itself
(or to the name param.txt asks for) */
fn transition_csv(project: &Path, notes: &mut Vec<String>) -> Option<String> {
    let wanted = project_parameters(project).and_then(|parameters| {
        parameters
            .get("transition_list")?
            .as_str()
            .map(str::to_string)
    });
    match &wanted {
        Some(name) if project_file(project, name).is_file() => return wanted,
        Some(_) => {}
        None => {
            if let Some(csv) = find_named_csv(project, &["transition"]) {
                return csv.file_name()?.to_str().map(str::to_string);
            }
        }
    }

    let (workbook, workbook_name) = transition_workbook(project)?;
    let target = match &wanted {
        Some(name) if name.to_ascii_lowercase().ends_with(".csv") => project_file(project, name),
        _ => workbook.with_extension("csv"),
    };
    let target_name = target.file_name()?.to_str()?.to_string();
    if !target.exists() {
        match convert_workbook(&workbook, &target) {
            Ok(rows) => notes.push(format!(
                "Converted {workbook_name} to {target_name} ({rows} transitions)."
            )),
            Err(error) => {
                notes.push(format!("{workbook_name} could not be converted: {error}."));
                return None;
            }
        }
    }
    Some(wanted.unwrap_or(target_name))
}

// a workbook named like a transition list, or the only workbook in the folder
fn transition_workbook(project: &Path) -> Option<(PathBuf, String)> {
    let workbooks = root_files(project)
        .into_iter()
        .filter(|(_, name)| {
            let lower = name.to_ascii_lowercase();
            // ~$ files are excel's lock files for an open workbook
            !lower.starts_with("~$")
                && [".xlsx", ".xlsm", ".xls"]
                    .iter()
                    .any(|extension| lower.ends_with(extension))
        })
        .collect::<Vec<_>>();
    let named = workbooks
        .iter()
        .position(|(_, name)| name.to_ascii_lowercase().contains("transition"));
    match (named, workbooks.len()) {
        (Some(index), _) => workbooks.into_iter().nth(index),
        (None, 1) => workbooks.into_iter().next(),
        _ => None,
    }
}

/* writes the transition sheet as csv, skipping blank rows. sciex workbooks can
hold the acquisition method next to the transitions, so the sheet whose header
has both a q1 and a q3 column wins, then one with a q1, then the first sheet.
returns the data row count */
fn convert_workbook(workbook: &Path, target: &Path) -> Result<usize, String> {
    let mut book = open_workbook_auto(workbook).map_err(|error| error.to_string())?;
    let (name, sheet) = book
        .worksheets()
        .into_iter()
        .enumerate()
        .max_by_key(|(index, (_, sheet))| (header_score(sheet), std::cmp::Reverse(*index)))
        .map(|(_, sheet)| sheet)
        .ok_or("the workbook has no sheets")?;
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    let mut rows = 0;
    for row in filled_rows(&sheet) {
        writer
            .write_record(row.iter().map(cell_text))
            .map_err(|error| error.to_string())?;
        rows += 1;
    }
    if rows < 2 {
        return Err(format!("the sheet {name} has no transitions"));
    }
    let bytes = writer.into_inner().map_err(|error| error.to_string())?;
    // written in one go so a failed conversion never leaves a half file behind
    fs::write(target, bytes).map_err(|error| error.to_string())?;
    Ok(rows - 1)
}

fn filled_rows(sheet: &Range<Data>) -> impl Iterator<Item = &[Data]> {
    sheet
        .rows()
        .filter(|row| row.iter().any(|cell| !matches!(cell, Data::Empty)))
}

// same header words the engine uses to find the q1 and q3 columns
fn header_score(sheet: &Range<Data>) -> usize {
    let Some(header) = filled_rows(sheet).next() else {
        return 0;
    };
    let headers = header
        .iter()
        .map(|cell| cell.to_string().to_lowercase())
        .collect::<Vec<_>>();
    let has = |keys: &[&str]| {
        headers
            .iter()
            .any(|header| keys.iter().any(|key| header.contains(key)))
    };
    match (
        has(&["q1", "precursor", "parent"]),
        has(&["q3", "fragment", "product", "daughter"]),
    ) {
        (true, true) => 2,
        (true, false) => 1,
        _ => 0,
    }
}

fn cell_text(cell: &Data) -> String {
    match cell {
        // 15 significant digits like excel shows, so 636.6167934150001 reads 636.616793415
        Data::Float(value) => format!("{value:.14e}")
            .parse::<f64>()
            .unwrap_or(*value)
            .to_string(),
        // headers like "XIC\nWidth (Da)" wrap inside the cell, kept on one line for the csv
        Data::String(text) => text.lines().map(str::trim).collect::<Vec<_>>().join(" "),
        Data::Error(_) => String::new(),
        cell => cell.to_string(),
    }
}

fn write_param(project: &Path, transitions: Option<&str>, notes: &mut Vec<String>) {
    let (duration, source) = match duration_from_names(&project.join(DATA_DIR)) {
        Some((seconds, file)) => (seconds, format!("from the name {file}")),
        None => (
            DEFAULT_DURATION,
            "the default, no duration in the file names".to_string(),
        ),
    };
    let list = toml::Value::String(transitions.unwrap_or(DEFAULT_TRANSITIONS).to_string());
    let text = format!(
        "transition_list = {list}\n\
         sample_duration = {duration}   # seconds of signal per ejection, {source}\n\
         # mass_tolerance = 0.03   # ± Da for every transition, replaces the XIC widths in the transition list\n"
    );
    match fs::write(project.join("param.txt"), text) {
        Ok(()) => notes.push(format!(
            "Created param.txt with sample_duration = {duration} s ({source})."
        )),
        Err(error) => notes.push(format!("param.txt could not be created: {error}.")),
    }
}

/* acquisition names often carry the ejection time, like
261023_ISTD_FROpt_10Hz_30s_240.wiff. the first "<number>s" token wins */
fn duration_from_names(data_dir: &Path) -> Option<(f64, String)> {
    root_files(data_dir).into_iter().find_map(|(_, name)| {
        let stem = name
            .strip_suffix(".wiff")
            .or_else(|| name.strip_suffix(".WIFF"))?;
        stem.split(['_', '-', ' '])
            .filter_map(|token| token.strip_suffix(['s', 'S'])?.parse::<f64>().ok())
            .find(|seconds| seconds.is_finite() && *seconds > 0.0)
            .map(|seconds| (seconds, name.clone()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn project(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("echomskit_setup_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        path
    }

    type Sheet<'a> = &'a [&'a [&'a str]];

    // a bare bones xlsx: each sheet holds inline strings and numbers
    fn write_workbook(path: &Path, sheets: &[Sheet]) {
        const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>"#;
        const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
        const RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
        const DOC_RELS: &str =
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let (mut types, mut names, mut links) = (String::new(), String::new(), String::new());
        let mut parts = Vec::new();
        for (number, rows) in (1..).zip(sheets) {
            types.push_str(&format!(
                r#"<Override PartName="/xl/worksheets/sheet{number}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
            ));
            names.push_str(&format!(
                r#"<sheet name="Sheet{number}" sheetId="{number}" r:id="rId{number}"/>"#
            ));
            links.push_str(&format!(
                r#"<Relationship Id="rId{number}" Type="{DOC_RELS}/worksheet" Target="worksheets/sheet{number}.xml"/>"#
            ));
            let mut sheet = format!(r#"{XML}<worksheet xmlns="{MAIN}"><sheetData>"#);
            for (row_number, row) in (1..).zip(rows.iter()) {
                sheet.push_str(&format!(r#"<row r="{row_number}">"#));
                for (column, value) in row.iter().enumerate() {
                    let at = format!("{}{row_number}", (b'A' + column as u8) as char);
                    if value.is_empty() {
                        continue;
                    }
                    if value.parse::<f64>().is_ok() {
                        sheet.push_str(&format!(r#"<c r="{at}"><v>{value}</v></c>"#));
                    } else {
                        sheet.push_str(&format!(
                            r#"<c r="{at}" t="inlineStr"><is><t>{value}</t></is></c>"#
                        ));
                    }
                }
                sheet.push_str("</row>");
            }
            sheet.push_str("</sheetData></worksheet>");
            parts.push((format!("xl/worksheets/sheet{number}.xml"), sheet));
        }
        parts.extend([
            (
                "[Content_Types].xml".to_string(),
                format!(
                    r#"{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>{types}</Types>"#
                ),
            ),
            (
                "_rels/.rels".to_string(),
                format!(
                    r#"{XML}<Relationships xmlns="{RELS}"><Relationship Id="rId1" Type="{DOC_RELS}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
                ),
            ),
            (
                "xl/workbook.xml".to_string(),
                format!(
                    r#"{XML}<workbook xmlns="{MAIN}" xmlns:r="{DOC_RELS}"><sheets>{names}</sheets></workbook>"#
                ),
            ),
            (
                "xl/_rels/workbook.xml.rels".to_string(),
                format!(r#"{XML}<Relationships xmlns="{RELS}">{links}</Relationships>"#),
            ),
        ]);
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, contents) in parts {
            zip.start_file(name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn raw_sciex_export_becomes_a_project() {
        let project = project("raw_export");
        for name in [
            "run_10Hz_30s_1.wiff",
            "run_10Hz_30s_1.wiff.scan",
            "run_10Hz_30s_1.wiff2",
        ] {
            fs::write(project.join(name), name).unwrap();
        }
        fs::write(project.join("run_10Hz_30s_1.mzML"), "").unwrap();
        // like the sciex workbook, the acquisition method sheet comes before the transitions
        write_workbook(
            &project.join("Trial Transitions.xlsx"),
            &[
                &[
                    &["Compound ID", "Precursor Ion (Da)", "Frag Start mass (Da)"],
                    &["TG 39:0", "698.6", "65"],
                ],
                &[
                    &[
                        "Name",
                        "Precursor (Q1)\nMass (Da)",
                        "Fragment (Q3) Mass (Da)",
                        "XIC Width (Da)",
                    ],
                    &["TG 39:0", "698.62932000000001", "", "0.02"],
                    &[],
                    &["TG 39:0 [NL]", "636.6167934150001", "467.4096", "0.05"],
                ],
            ],
        );

        let notes = prepare(&project);
        assert!(
            notes
                .iter()
                .any(|note| note == "Moved 3 SCIEX file(s) into wiff_dir."),
            "{notes:?}"
        );
        assert!(
            notes.iter().any(|note| note.contains("(2 transitions)")),
            "{notes:?}"
        );
        assert!(project.join("wiff_dir/run_10Hz_30s_1.wiff.scan").is_file());
        assert!(project.join("run_10Hz_30s_1.mzML").is_file());
        assert_eq!(
            fs::read_to_string(project.join("Trial Transitions.csv")).unwrap(),
            "Name,Precursor (Q1) Mass (Da),Fragment (Q3) Mass (Da),XIC Width (Da)\n\
             TG 39:0,698.62932,,0.02\n\
             TG 39:0 [NL],636.616793415,467.4096,0.05\n"
        );
        let param = fs::read_to_string(project.join("param.txt")).unwrap();
        let parameters = param.parse::<toml::Table>().unwrap();
        assert_eq!(
            parameters["transition_list"].as_str(),
            Some("Trial Transitions.csv")
        );
        assert_eq!(parameters["sample_duration"].as_integer(), Some(30));

        // a second scan finds nothing left to do
        assert!(prepare(&project).is_empty());
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn existing_files_are_never_overwritten() {
        let project = project("existing");
        fs::create_dir(project.join(DATA_DIR)).unwrap();
        fs::write(project.join(DATA_DIR).join("a.wiff"), "kept").unwrap();
        fs::write(project.join("a.wiff"), "loose").unwrap();
        fs::write(project.join("param.txt"), "transition_list = 'mine.csv'\n").unwrap();
        fs::write(project.join("mine.csv"), "Name,Q1,Q3\n").unwrap();
        write_workbook(
            &project.join("transitions.xlsx"),
            &[&[&["Name", "Q1"], &["x", "1"]]],
        );

        let notes = prepare(&project);
        assert_eq!(notes, ["Left a.wiff in place, wiff_dir already has one."]);
        assert_eq!(
            fs::read_to_string(project.join(DATA_DIR).join("a.wiff")).unwrap(),
            "kept"
        );
        assert_eq!(
            fs::read_to_string(project.join("param.txt")).unwrap(),
            "transition_list = 'mine.csv'\n"
        );
        assert!(!project.join("transitions.csv").exists());
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn duration_falls_back_without_a_seconds_token() {
        let project = project("no_duration");
        fs::create_dir(project.join(DATA_DIR)).unwrap();
        fs::write(project.join(DATA_DIR).join("plate_10Hz_ms2.wiff"), "").unwrap();
        assert_eq!(duration_from_names(&project.join(DATA_DIR)), None);
        fs::write(project.join(DATA_DIR).join("b_12.5s.wiff"), "").unwrap();
        assert_eq!(
            duration_from_names(&project.join(DATA_DIR)),
            Some((12.5, "b_12.5s.wiff".to_string()))
        );
        prepare(&project);
        let param = fs::read_to_string(project.join("param.txt")).unwrap();
        assert!(param.contains("sample_duration = 12.5"), "{param}");
        assert!(
            param.contains("transition_list = \"transitions.csv\""),
            "{param}"
        );
        fs::remove_dir_all(&project).unwrap();
    }
}
