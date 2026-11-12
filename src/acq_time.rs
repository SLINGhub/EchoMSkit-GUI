use crate::common::{ACQ_TIME, Acq, DATA_DIR, Param, data_files, duration_tag, read_acq_time};
use crate::wiff::{Acquisition, read_acquisition};
use std::error::Error;
use std::path::Path;

/* finds one window per ejection from the tic and writes acq_time.csv. windows
already in acq_time.csv are kept so edited bounds survive a rerun, unless the
sample duration changed since they were detected */
pub fn read(param_t: &Param) -> Result<(), Box<dyn Error>> {
    let mut previous = match read_acq_time() {
        Ok((Some(duration), acq_time)) if duration == param_t.sample_duration => acq_time,
        _ => Vec::new(),
    };
    let mut acq_time = Vec::new();
    for file in data_files()? {
        if let Some(index) = previous.iter().position(|acq| acq.file == file) {
            acq_time.push(previous.swap_remove(index));
            continue;
        }
        let acquisition = read_acquisition(&Path::new(DATA_DIR).join(&file))
            .map_err(|error| format!("{file}: {error}"))?;
        let (windows, placed) = detect(&acquisition, param_t);
        if !placed.is_empty() {
            let names = placed
                .iter()
                .map(|&index| format!("{} {}", index + 1, acquisition.wells[index]))
                .collect::<Vec<_>>()
                .join(", ");
            println!("{file:?}: no signal for {names}, placed from the ejection spacing");
        }
        let windows = label(&file, &acquisition.wells, windows);
        println!("{file:?}: {} samples", windows.len());
        acq_time.push(Acq { file, windows });
    }
    write_acq_time(&acq_time, param_t.sample_duration)
}

fn write_acq_time(acq_time: &[Acq], duration: f32) -> Result<(), Box<dyn Error>> {
    let mut text = duration_tag(duration);
    text.push('\n');
    let mut wtr = csv::WriterBuilder::new().from_writer(text.into_bytes());
    for Acq { file, windows } in acq_time {
        wtr.write_record([file.as_str(), "", ""])?;
        for (label, beg, end) in windows {
            wtr.write_record([label, &format!("{beg:.3}"), &format!("{end:.3}")])?;
        }
    }
    std::fs::write(ACQ_TIME, wtr.into_inner()?)?;
    Ok(())
}

fn label(file: &str, wells: &[String], windows: Vec<(f32, f32)>) -> Vec<(String, f32, f32)> {
    let named = wells.len() == windows.len();
    if !named && !wells.is_empty() {
        println!(
            "{file:?}: found {} samples but the echo well list has {}, numbering them instead",
            windows.len(),
            wells.len()
        );
    }
    // a blank plus a stray plateau still matches the well count, uneven spacing gives it away
    if named
        && windows.len() > 2
        && ejection_steps(&windows).is_none_or(|steps| steps.iter().any(|&step| step > 1))
    {
        println!("{file:?}: the ejections are unevenly spaced, check the well labels line up");
    }
    windows
        .into_iter()
        .enumerate()
        .map(|(index, (beg, end))| {
            let label = if named {
                format!("{} {}", index + 1, wells[index])
            } else {
                (index + 1).to_string()
            };
            (label, beg, end)
        })
        .collect()
}

/* each ejection shows up as a plateau in the per cycle tic. cycles a quarter of
the way from the low to the high tic level form plateaus, and plateaus spanning
several sample durations (back to back ejections) are split evenly. each window
is the sample duration plus a scan cycle on both sides. returns the windows and
the indices of any placed by fill_blanks */
fn detect(acquisition: &Acquisition, param_t: &Param) -> (Vec<(f32, f32)>, Vec<usize>) {
    let duration = param_t.sample_duration / 60.0;
    let cycles = acquisition
        .scans
        .chunks(acquisition.experiments.len())
        .map(|cycle| {
            let time = (cycle[0].time + cycle[cycle.len() - 1].time) / 2.0;
            let tic = cycle.iter().map(|scan| scan.tic).sum::<f64>() / cycle.len() as f64;
            (time, tic)
        })
        .collect::<Vec<_>>();
    if cycles.len() < 2 {
        return (Vec::new(), Vec::new());
    }

    let mut levels = cycles.iter().map(|cycle| cycle.1).collect::<Vec<_>>();
    levels.sort_unstable_by(f64::total_cmp);
    let percentile =
        |fraction: f64| levels[((levels.len() - 1) as f64 * fraction).round() as usize];
    let (low, high) = (percentile(0.05), percentile(0.95));
    if high <= low {
        return (Vec::new(), Vec::new());
    }
    // a quarter of the way up so weaker ejections still count, the reference
    // run finds the same windows at anything from 0.15 to 0.5
    let threshold = low + (high - low) / 4.0;
    let period = (cycles[cycles.len() - 1].0 - cycles[0].0) / (cycles.len() - 1) as f32;

    let mut windows = Vec::new();
    let mut at = 0;
    while at < cycles.len() {
        if cycles[at].1 < threshold {
            at += 1;
            continue;
        }
        let first = at;
        while at < cycles.len() && cycles[at].1 >= threshold {
            at += 1;
        }
        let beg = cycles[first].0 - period / 2.0;
        let span = cycles[at - 1].0 + period / 2.0 - beg;
        if span < duration / 3.0 {
            continue;
        }
        let pieces = (span / duration).round().max(1.0);
        for piece in 0..pieces as usize {
            let center = beg + (piece as f32 + 0.5) * span / pieces;
            windows.push((center - duration / 2.0, center + duration / 2.0));
        }
    }
    let placed = fill_blanks(&mut windows, acquisition.wells.len(), duration);
    widen(&mut windows, period);
    (windows, placed)
}

/* the plateaus come from the summed tic, so a transition scanned late in the
cycle can start rising a cycle before the window does. a cycle of margin on each
side keeps both ramps inside, and windows that would then overlap (back to back
ejections) meet halfway instead so no scan is counted twice */
fn widen(windows: &mut [(f32, f32)], margin: f32) {
    for window in windows.iter_mut() {
        window.0 -= margin;
        window.1 += margin;
    }
    for index in 1..windows.len() {
        if windows[index - 1].1 > windows[index].0 {
            let middle = (windows[index - 1].1 + windows[index].0) / 2.0;
            windows[index - 1].1 = middle;
            windows[index].0 = middle;
        }
    }
}

/* how many ejection spacings each gap between windows spans, measured against
the lower median gap. none when a gap is not close to a whole number of spacings
(or there are fewer than two windows), since then the pace cannot be trusted */
fn ejection_steps(windows: &[(f32, f32)]) -> Option<Vec<usize>> {
    let gaps = windows
        .windows(2)
        .map(|pair| (pair[1].0 + pair[1].1 - pair[0].0 - pair[0].1) / 2.0)
        .collect::<Vec<_>>();
    let mut sorted = gaps.clone();
    sorted.sort_unstable_by(f32::total_cmp);
    let guess = sorted[gaps.len().checked_sub(1)? / 2];
    gaps.iter()
        .map(|gap| {
            let ratio = gap / guess;
            // written so a nan ratio fails as well
            (ratio >= 0.75 && (ratio - ratio.round()).abs() <= 0.25)
                .then_some(ratio.round() as usize)
        })
        .collect()
}

/* a blank well never lifts the tic, so its ejection is missing from the detected
windows. the echo ejects at a steady pace, so when gaps between windows span
whole extra spacings and those account for every missing well, windows are
placed evenly inside them. blanks at either end of the run are left alone, the
dead time around a run says nothing reliable about which end they were at, so
those windows get numbered instead. returns the indices of the placed windows */
fn fill_blanks(windows: &mut Vec<(f32, f32)>, wells: usize, duration: f32) -> Vec<usize> {
    let missing = wells.saturating_sub(windows.len());
    // blanks have to be outnumbered by measured gaps for the median to be the pace
    if missing == 0 || missing + 1 >= windows.len() {
        return Vec::new();
    }
    let Some(steps) = ejection_steps(windows) else {
        return Vec::new();
    };
    if steps.iter().map(|step| step - 1).sum::<usize>() != missing {
        return Vec::new();
    }

    let centers = windows
        .iter()
        .map(|(beg, end)| (beg + end) / 2.0)
        .collect::<Vec<_>>();
    let mut filled = vec![centers[0]];
    let mut placed = Vec::with_capacity(missing);
    for (pair, &step) in centers.windows(2).zip(&steps) {
        for part in 1..step {
            placed.push(filled.len());
            filled.push(pair[0] + part as f32 * (pair[1] - pair[0]) / step as f32);
        }
        filled.push(pair[1]);
    }
    *windows = filled
        .iter()
        .map(|center| (center - duration / 2.0, center + duration / 2.0))
        .collect();
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(centers: &[f32]) -> Vec<(f32, f32)> {
        centers
            .iter()
            .map(|&center| (center - 0.1, center + 0.1))
            .collect()
    }

    fn centers(windows: &[(f32, f32)]) -> Vec<f32> {
        windows
            .iter()
            .map(|(beg, end)| ((beg + end) * 500.0).round() / 1000.0)
            .collect()
    }

    #[test]
    fn blanks_between_ejections_are_placed_evenly() {
        let mut found = windows(&[1.0, 2.0, 4.0, 5.0, 8.0]);
        assert_eq!(fill_blanks(&mut found, 8, 0.2), [2, 5, 6]);
        assert_eq!(centers(&found), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn unclear_spacing_or_end_blanks_are_left_alone() {
        let cases: [(&[f32], usize); 6] = [
            // blank at either end, nothing in the gaps accounts for it
            (&[1.0, 2.0, 3.0, 4.0], 5),
            // a gap of one and a half spacings
            (&[1.0, 2.0, 3.5, 4.5], 5),
            // as many blanks as measured gaps
            (&[1.0, 3.0, 5.0], 5),
            // more windows than wells, or already matching
            (&[1.0, 2.0, 3.0], 2),
            (&[1.0, 2.0, 3.0], 3),
            (&[1.0, f32::NAN, 3.0, 4.0], 5),
        ];
        for (found, wells) in cases {
            let mut windows = windows(found);
            let before = format!("{windows:?}");
            assert!(
                fill_blanks(&mut windows, wells, 0.2).is_empty(),
                "{found:?}"
            );
            assert_eq!(format!("{windows:?}"), before);
        }
        assert!(fill_blanks(&mut Vec::new(), 3, 0.2).is_empty());
        assert!(fill_blanks(&mut windows(&[1.0]), 3, 0.2).is_empty());
    }

    #[test]
    fn widened_windows_meet_halfway_when_they_would_overlap() {
        let mut found = vec![(1.0, 2.0), (2.0, 3.0), (5.0, 6.0)];
        widen(&mut found, 0.25);
        assert_eq!(found, [(0.75, 2.0), (2.0, 3.25), (4.75, 6.25)]);
    }

    #[test]
    fn spacing_check_flags_stray_or_missing_ejections() {
        assert_eq!(
            ejection_steps(&windows(&[1.0, 2.0, 3.1, 4.0])),
            Some(vec![1, 1, 1])
        );
        assert_eq!(ejection_steps(&windows(&[1.0, 2.0, 4.0])), Some(vec![1, 2]));
        assert_eq!(ejection_steps(&windows(&[1.0, 2.0, 2.4, 3.0])), None);
        assert_eq!(ejection_steps(&windows(&[1.0])), None);
    }
}
