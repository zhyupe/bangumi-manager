use super::Analysis;
use crate::{db::Store, model::*, musicbrainz, providers};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use sha1::{Digest, Sha1};

#[derive(Serialize)]
pub struct Candidates {
    pub method: String,
    pub items: Vec<RemoteSubject>,
    pub next_offset: Option<u32>,
}
pub fn disc_id(offsets: &[u32], leadout: u32) -> Result<String> {
    if offsets.is_empty()
        || offsets.len() > 99
        || offsets.windows(2).any(|v| v[0] >= v[1])
        || offsets.last().unwrap() >= &leadout
    {
        return Err("Invalid CD TOC".into());
    }
    let mut input = format!("01{:02X}{leadout:08X}", offsets.len());
    for i in 0..99 {
        input += &format!("{:08X}", offsets.get(i).copied().unwrap_or(0));
    }
    Ok(STANDARD
        .encode(Sha1::digest(input.as_bytes()))
        .replace('+', ".")
        .replace('/', "_")
        .replace('=', "-"))
}
pub fn candidates(
    store: &Store,
    id: &str,
    query: Option<String>,
    offset: u32,
) -> Result<Candidates> {
    let a: Analysis = store.get("cue_analyses", id)?.ok_or("Analyze CUE first")?;
    if let Some(query) = query.filter(|s| !s.trim().is_empty()) {
        let page = musicbrainz::releases(store, &query, offset, false)?;
        return Ok(Candidates {
            method: "text".into(),
            items: page.items,
            next_offset: page.next_offset,
        });
    }
    if let Some(id) = a
        .sheet
        .rem
        .get("MUSICBRAINZ_ALBUMID")
        .or(a.sheet.rem.get("MUSICBRAINZ_RELEASEID"))
    {
        return Ok(Candidates {
            method: "mbid".into(),
            items: vec![musicbrainz::detail(store, id, false, None)?],
            next_offset: None,
        });
    }
    // Only a complete single-file CD image is treated as an exact TOC.
    // Multi-file images and virtual gaps use the observed audio timeline for a near match.
    if !a.sources.is_empty() {
        let source = &a.sources[0];
        let total_samples = a
            .sources
            .iter()
            .try_fold(0u64, |n, s| n.checked_add(s.samples))
            .ok_or("Invalid CD TOC")?;
        let leadout = u32::try_from(total_samples as u128 * 75 / source.rate as u128 + 150)
            .map_err(|_| "Invalid CD TOC")?;
        let offsets = a
            .sheet
            .tracks
            .iter()
            .map(|t| {
                let preceding = a.sources[..t.file]
                    .iter()
                    .map(|s| s.samples as u128)
                    .sum::<u128>();
                u32::try_from(
                    preceding * 75 / source.rate as u128 + t.index01.unwrap() as u128 + 150,
                )
                .map_err(|_| "Invalid CD TOC".to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        let exact = a.sources.len() == 1
            && a.sheet
                .tracks
                .iter()
                .all(|t| t.pregap == 0 && t.postgap == 0)
            && !source.lossy
            && source.rate == 44100
            && source.bits == 16
            && source.channels == 2
            && source.samples % 588 == 0;
        let discid = if exact {
            disc_id(&offsets, leadout)?
        } else {
            "-".into()
        };
        let toc = std::iter::once("1".to_string())
            .chain([offsets.len().to_string(), leadout.to_string()])
            .chain(offsets.iter().map(u32::to_string))
            .collect::<Vec<_>>()
            .join(" ");
        let mut url =
            url::Url::parse(&format!("https://musicbrainz.org/ws/2/discid/{discid}")).unwrap();
        url.query_pairs_mut().extend_pairs([
            ("fmt", "json"),
            ("inc", "artist-credits+recordings+release-groups"),
            ("cdstubs", "no"),
            ("toc", &toc),
        ]);
        match providers::request(store, url.as_str(), None, false) {
            Ok((v, stale)) => {
                let items = v["releases"]
                    .as_array()
                    .map(|r| {
                        r.iter()
                            .map(|v| musicbrainz::project(v, stale))
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()?
                    .unwrap_or_default();
                if !items.is_empty() {
                    let matched = exact && v["id"].as_str() == Some(discid.as_str());
                    return Ok(Candidates {
                        method: if matched { "discid" } else { "toc" }.into(),
                        items,
                        next_offset: None,
                    });
                }
            }
            Err(e) if e.contains("404") => (),
            Err(e) => return Err(e),
        }
    }
    let query = format!("{} {}", a.sheet.title, a.sheet.performer);
    let page = musicbrainz::releases(store, query.trim(), 0, false)?;
    Ok(Candidates {
        method: "text".into(),
        items: page.items,
        next_offset: page.next_offset,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashes_cd_toc_using_musicbrainz_encoding() {
        assert_eq!(
            disc_id(&[150, 15150, 30150], 45150).unwrap(),
            "MXsnrcgOkbb2H34IbwspNVMJMwA-"
        );
        assert!(disc_id(&[150, 150], 1000).is_err());
        let value = disc_id(&[150, 15363, 32314, 46592, 65831, 80489], 95462).unwrap();
        assert!(!value.contains(['+', '/', '=']));
    }
    #[test]
    fn exact_and_approximate_toc_queries_keep_multiple_releases_without_queue_lock() {
        use super::super::{analysis::AudioSource, parser::parse, AnalyzeRequest};
        use serde_json::json;
        for lossy in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let store = Store::open(dir.path().into()).unwrap();
            let a=Analysis {id:"test".into(),request:AnalyzeRequest::default(),cue:VerifiedFile {path:"album.cue".into(),signature:Signature {size:0,modified:0},digest:String::new()},sheet:parse("REM DISCID 1234abcd\nFILE \"source.flac\" WAVE\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 03:20:00").unwrap(),sources:vec![AudioSource {file:VerifiedFile {path:"source.flac".into(),signature:Signature {size:0,modified:0},digest:String::new()},codec:"flac".into(),rate:44100,channels:2,bits:16,samples:588*45000,lossy}],tracks:vec![],warnings:vec![]};
            store.put("cue_analyses", &a.id, &a).unwrap();
            let disc = if lossy {
                "-".into()
            } else {
                disc_id(&[150, 15150], 45150).unwrap()
            };
            let mut url =
                url::Url::parse(&format!("https://musicbrainz.org/ws/2/discid/{disc}")).unwrap();
            url.query_pairs_mut().extend_pairs([
                ("fmt", "json"),
                ("inc", "artist-credits+recordings+release-groups"),
                ("cdstubs", "no"),
                ("toc", "1 2 45150 150 15150"),
            ]);
            store.cache_put(&format!("{url}:"),&json!({"id":disc,"releases":[{"id":"00000000-0000-4000-8000-000000000001","title":"Example Album A"},{"id":"00000000-0000-4000-8000-000000000002","title":"Example Album B"}]})).unwrap();
            let queue = store.queue_lock.lock().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let worker = store.clone();
            let thread =
                std::thread::spawn(move || tx.send(candidates(&worker, "test", None, 0)).unwrap());
            let result = rx.recv_timeout(std::time::Duration::from_secs(2));
            drop(queue);
            thread.join().unwrap();
            let result = result.unwrap().unwrap();
            assert_eq!(result.items.len(), 2);
            assert_eq!(result.method, if lossy { "toc" } else { "discid" });
        }
    }
}
