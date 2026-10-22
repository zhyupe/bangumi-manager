use crate::model::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CueSheet {
    pub title: String,
    pub performer: String,
    pub rem: BTreeMap<String, String>,
    pub files: Vec<String>,
    pub tracks: Vec<CueTrack>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CueTrack {
    pub number: u32,
    pub file: usize,
    pub title: String,
    pub performer: String,
    pub isrc: String,
    pub index00: Option<u64>,
    pub index01: Option<u64>,
    pub pregap: u64,
    pub postgap: u64,
}

pub fn decode(bytes: &[u8], encoding: &str) -> Result<String> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("CUE exceeds 4 MiB".into());
    }
    let (codec, skip) = if let Some((codec, skip)) = encoding_rs::Encoding::for_bom(bytes) {
        (codec, skip)
    } else if encoding.is_empty() || encoding == "utf-8" {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "CUE text encoding is unknown; choose GB18030 or Shift-JIS")?;
        if text.contains('\u{fffd}') {
            return Err("CUE contains invalid characters for this encoding".into());
        }
        return Ok(text.into());
    } else {
        let codec = match encoding {
            "gb18030" => encoding_rs::GB18030,
            "shift-jis" => encoding_rs::SHIFT_JIS,
            _ => return Err("Unsupported CUE text encoding".into()),
        };
        (codec, 0)
    };
    let (text, errors) = codec.decode_without_bom_handling(&bytes[skip..]);
    if errors || text.contains('\u{fffd}') {
        return Err("CUE contains invalid characters for this encoding".into());
    }
    Ok(text.into_owned())
}
fn words(line: &str) -> Result<Vec<String>> {
    let mut words = vec![];
    let mut current = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if quoted {
        return Err("Unclosed CUE quotation".into());
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}
fn frames(raw: &str) -> Result<u64> {
    let fields = raw
        .split(':')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| "Invalid CUE time")?;
    if fields.len() != 3 || fields[1] >= 60 || fields[2] >= 75 || fields[0] > 9999 {
        return Err("Invalid CUE time".into());
    }
    Ok((fields[0] * 60 + fields[1]) * 75 + fields[2])
}
pub fn sample_at(frame: u64, rate: u32) -> Result<u64> {
    frame
        .checked_mul(rate as u64)
        .and_then(|v| v.checked_add(37))
        .map(|v| v / 75)
        .ok_or_else(|| "CUE sample offset overflow".into())
}
pub fn parse(text: &str) -> Result<CueSheet> {
    let mut sheet = CueSheet::default();
    for (line_no, line) in text.lines().enumerate() {
        let result = (|| -> Result<()> {
            let words = words(line.trim_start_matches('\u{feff}'))?;
            if words.is_empty() {
                return Ok(());
            }
            let arg = |n: usize| {
                words
                    .get(n)
                    .map(String::as_str)
                    .ok_or("Missing CUE argument".to_string())
            };
            match words[0].to_ascii_uppercase().as_str() {
                "FILE" => {
                    if words.len() != 3
                        || !["WAVE", "MP3", "AIFF", "FLAC"]
                            .contains(&arg(2)?.to_ascii_uppercase().as_str())
                    {
                        return Err("Only audio FILE entries are supported".into());
                    }
                    let name = arg(1)?;
                    if sheet.files.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                        return Err("Repeated CUE FILE entries are unsupported".into());
                    }
                    sheet.files.push(name.into());
                }
                "TRACK" => {
                    let number: u32 = arg(1)?.parse().map_err(|_| "Invalid CUE track number")?;
                    if arg(2)?.to_ascii_uppercase() != "AUDIO" {
                        return Err("Data tracks are unsupported".into());
                    }
                    if sheet.files.is_empty()
                        || number == 0
                        || number > 99
                        || sheet.tracks.last().is_some_and(|t| t.number + 1 != number)
                    {
                        return Err("CUE track numbers must be consecutive within 1–99".into());
                    }
                    sheet.tracks.push(CueTrack {
                        number,
                        file: sheet.files.len() - 1,
                        ..Default::default()
                    });
                }
                "TITLE" | "PERFORMER" => {
                    let value = words[1..].join(" ");
                    let title = words[0].eq_ignore_ascii_case("TITLE");
                    if let Some(track) = sheet.tracks.last_mut() {
                        if title {
                            track.title = value;
                        } else {
                            track.performer = value;
                        }
                    } else if title {
                        sheet.title = value;
                    } else {
                        sheet.performer = value;
                    }
                }
                "INDEX" => {
                    let number = arg(1)?;
                    let time = frames(arg(2)?)?;
                    let track = sheet.tracks.last_mut().ok_or("INDEX before TRACK")?;
                    if track.file != sheet.files.len() - 1 {
                        return Err("INDEX before TRACK in this FILE".into());
                    }
                    let slot = match number {
                        "00" => &mut track.index00,
                        "01" => &mut track.index01,
                        _ => return Err("Only INDEX 00 and 01 are supported".into()),
                    };
                    if slot.replace(time).is_some() {
                        return Err("Duplicate CUE index".into());
                    }
                }
                "PREGAP" | "POSTGAP" => {
                    let duration = frames(arg(1)?)?;
                    let track = sheet.tracks.last_mut().ok_or("Gap before TRACK")?;
                    if words[0].eq_ignore_ascii_case("PREGAP") {
                        track.pregap = duration;
                    } else {
                        track.postgap = duration;
                    }
                    sheet
                        .warnings
                        .push("Declared gaps without stored audio are not synthesized".into());
                }
                "ISRC" => sheet.tracks.last_mut().ok_or("ISRC before TRACK")?.isrc = arg(1)?.into(),
                "REM" => {
                    let key = arg(1)?.to_ascii_uppercase();
                    let value = words[2..].join(" ");
                    if ["PRE_EMPHASIS", "PRE-EMPHASIS", "PREEMPHASIS"].contains(&key.as_str())
                        && !["NO", "OFF", "FALSE", "0"]
                            .contains(&value.to_ascii_uppercase().as_str())
                    {
                        return Err(
                            "Pre-emphasis and unsupported audio flags require external processing"
                                .into(),
                        );
                    }
                    sheet.rem.insert(key, value);
                }
                "FLAGS" => {
                    if words.iter().skip(1).any(|v| !v.eq_ignore_ascii_case("DCP")) {
                        return Err(
                            "Pre-emphasis and unsupported audio flags require external processing"
                                .into(),
                        );
                    }
                }
                "CATALOG" => {
                    sheet.rem.insert("CATALOG".into(), arg(1)?.into());
                }
                _ => return Err(format!("Unsupported CUE directive: {}", words[0])),
            }
            Ok(())
        })();
        result.map_err(|e| format!("CUE line {}: {e}", line_no + 1))?;
    }
    if sheet.tracks.is_empty() || sheet.tracks[0].number != 1 {
        return Err("CUE must begin with audio track 1".into());
    }
    for (i, track) in sheet.tracks.iter().enumerate() {
        let start = track.index01.ok_or("Each CUE track requires INDEX 01")?;
        if track.index00.is_some_and(|v| v > start) {
            return Err("INDEX 00 follows INDEX 01".into());
        }
        if i > 0 && sheet.tracks[i - 1].file == track.file {
            let previous = sheet.tracks[i - 1].index01.unwrap();
            if start <= previous || track.index00.is_some_and(|v| v < previous) {
                return Err("CUE track boundaries overlap or are out of order".into());
            }
        }
    }
    if (0..sheet.files.len()).any(|f| !sheet.tracks.iter().any(|t| t.file == f)) {
        return Err("CUE contains an audio file without tracks".into());
    }
    sheet.warnings.sort();
    sheet.warnings.dedup();
    Ok(sheet)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_multifile_indices_and_does_not_confuse_discid() {
        let sheet = parse("TITLE \"Fictional Album\"\nREM DISCID ABCD1234\nFILE \"disc one.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 00 00:00:00\nINDEX 01 00:00:30\nTRACK 02 AUDIO\nINDEX 00 01:00:00\nINDEX 01 01:02:00\nFILE \"disc two.flac\" WAVE\nTRACK 03 AUDIO\nINDEX 01 00:00:00").unwrap();
        assert_eq!(sheet.tracks[1].index01, Some(4650));
        assert_eq!(sheet.tracks[2].file, 1);
        assert!(!sheet.rem.contains_key("MUSICBRAINZ_ALBUMID"));
        assert_eq!(sample_at(1, 44100).unwrap(), 588);
    }
    #[test]
    fn rejects_invalid_boundaries_and_non_audio() {
        for tail in [
            "TRACK 01 MODE1/2352\nINDEX 01 00:00:00",
            "TRACK 01 AUDIO\nINDEX 01 00:60:00",
            "TRACK 01 AUDIO\nFLAGS PRE\nINDEX 01 00:00:00",
            "TRACK 01 AUDIO\nINDEX 00 00:01:00\nINDEX 01 00:00:00",
            "TRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 00:00:00",
        ] {
            assert!(parse(&format!("FILE \"a.wav\" WAVE\n{tail}")).is_err());
        }
    }
    #[test]
    fn strictly_decodes_legacy_text_and_bom() {
        let (bytes, _, _) = encoding_rs::GB18030.encode("虚构专辑");
        assert!(decode(&bytes, "").is_err());
        assert_eq!(decode(&bytes, "gb18030").unwrap(), "虚构专辑");
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("架空の音楽");
        assert_eq!(decode(&bytes, "shift-jis").unwrap(), "架空の音楽");
        assert_eq!(decode(b"\xef\xbb\xbfTITLE", "").unwrap(), "TITLE");
        assert!(decode(&[0xff], "utf-8").is_err());
    }
}
