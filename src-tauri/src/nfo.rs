use crate::model::*;
use quick_xml::{events::Event, Reader};
use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

#[derive(Debug, Default, Clone)]
pub struct Node {
    pub name: String,
    pub text: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Node>,
}
impl Node {
    pub fn value(&self, name: &str) -> String {
        self.children
            .iter()
            .find(|n| n.name == name)
            .map(|n| n.text.trim().to_string())
            .unwrap_or_default()
    }
    pub fn number(&self, name: &str) -> Option<u32> {
        self.value(name).parse().ok()
    }
    pub fn ids(&self) -> Result<BTreeMap<String, String>> {
        let mut ids = BTreeMap::new();
        for child in &self.children {
            let provider = if child.name == "uniqueid"
                || child.name == "id" && child.attrs.contains_key("type")
            {
                child.attrs.get("type").map(String::as_str).unwrap_or("")
            } else {
                match child.name.as_str() {
                    "tmdbid" => "tmdb",
                    "tvdbid" => "tvdb",
                    "imdbid" => "imdb",
                    "id" if child.text.trim().starts_with("tt") => "imdb",
                    "bangumiid" => "bangumi",
                    "musicbrainzalbumid" | "musicbrainztrackid" => "musicbrainz",
                    "musicbrainzreleasegroupid" => "musicbrainz_releasegroup",
                    "musicbrainzreleasetrackid" => "musicbrainz_track",
                    _ => continue,
                }
            }
            .to_lowercase();
            if !["tmdb", "tvdb", "imdb", "bangumi", "musicbrainz", "musicbrainz_releasegroup", "musicbrainz_track"].contains(&provider.as_str()) {
                continue;
            }
            let value = child.text.trim();
            if value.is_empty() {
                continue;
            }
            let value = if provider.starts_with("musicbrainz") { crate::musicbrainz::mbid(value)? } else { value.to_string() };
            if ["tmdb", "tvdb", "bangumi"].contains(&provider.as_str())
                && (!value.chars().all(|c| c.is_ascii_digit())
                    || value.parse::<u64>().unwrap_or(0) == 0)
            {
                return Err(format!("无效的 {provider} ID：{value}"));
            }
            if provider == "imdb"
                && !value
                    .strip_prefix("tt")
                    .is_some_and(|id| !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()))
            {
                return Err(format!("无效的 imdb ID：{value}"));
            }
            if let Some(old) = ids.insert(provider.clone(), value.to_string()) {
                if old != value {
                    return Err(format!("NFO 中的 {provider} ID 冲突：{old} / {value}"));
                }
            }
        }
        Ok(ids)
    }
    pub fn info(&self, language: &str) -> Result<MediaInfo> {
        let mut info = MediaInfo {
            external_ids: self.ids()?,
            localized_title: self.value("localizedtitle"),
            original_title: self.value("originaltitle"),
            language: self.value("language"),
            original_language: self.value("originallanguage"),
            overview: self.value("plot"),
            original_overview: self.value("originalplot"),
            air_date: self.value("premiered"),
            artist: self.value("artist"),
            ..Default::default()
        };
        if info.localized_title.is_empty() {
            info.localized_title = self.value("title");
        }
        if info.language.is_empty() {
            info.language = language.into();
        }
        if info.overview.is_empty() {
            info.overview = self.value("outline");
        }
        if info.air_date.is_empty() {
            info.air_date = self.value("aired");
        }
        if info.air_date.is_empty() {
            info.air_date = self.value("releasedate");
        }
        info.countries = self
            .children
            .iter()
            .filter(|n| n.name == "country")
            .map(|n| n.text.trim().into())
            .collect();
        if self
            .children
            .iter()
            .any(|n| n.name == "genre" && ["动画", "Animation", "アニメ"].contains(&n.text.trim()))
        {
            info.genres.push(16);
        }
        info.poster = self
            .children
            .iter()
            .find(|n| n.name == "thumb" && n.attrs.get("aspect").is_none_or(|v| v == "poster"))
            .map(|n| n.text.trim().to_string())
            .filter(|s| s.starts_with("https://"))
            .unwrap_or_default();
        info.backdrop = self
            .children
            .iter()
            .find(|n| n.name == "fanart")
            .map(|n| n.value("thumb"))
            .filter(|s| s.starts_with("https://"))
            .unwrap_or_default();
        Ok(info)
    }
}
// Accept adjacent episodedetails roots for a multi-episode video. Only direct
// children are used for identity, so an actor's tmdbid is never the work ID.
pub fn parse(raw: &str) -> Result<Vec<Node>> {
    let mut reader = Reader::from_str(raw.trim_start_matches('\u{feff}'));
    let mut stack: Vec<Node> = vec![];
    let mut roots = vec![];
    let mut count = 0;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("NFO XML 无效：{e}"))?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let name = e.name().as_ref().to_lowercase();
                let mut node = Node {
                    name,
                    ..Default::default()
                };
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| e.to_string())?;
                    node.attrs.insert(
                        attr.key.as_ref().to_lowercase(),
                        attr.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                            .map_err(|e| e.to_string())?
                            .into_owned(),
                    );
                }
                count += 1;
                if stack.len() >= 64 || count > 100_000 {
                    return Err("NFO XML 过于复杂".into());
                }
                if empty {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        roots.push(node);
                    }
                } else {
                    stack.push(node);
                }
            }
            Event::End(_) => {
                let node = stack.pop().ok_or("NFO XML 标签未配对")?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    roots.push(node);
                }
            }
            Event::Text(e) => {
                let text = e.to_string();
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err("NFO 根节点外含有无效内容".into());
                }
            }
            Event::CData(e) => {
                let node = stack.last_mut().ok_or("NFO CDATA 不在节点内")?;
                node.text.push_str(&e.to_string());
            }
            Event::GeneralRef(e) => {
                let escaped = format!("&{};", e.to_string());
                let text = quick_xml::escape::unescape(&escaped).map_err(|e| e.to_string())?;
                stack
                    .last_mut()
                    .ok_or("NFO 实体不在节点内")?
                    .text
                    .push_str(&text);
            }
            Event::DocType(_) => return Err("NFO 不支持 DTD 或外部实体".into()),
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() || roots.is_empty() {
        return Err("NFO XML 不完整".into());
    }
    Ok(roots)
}
pub fn read(path: &Path) -> Result<Vec<Node>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("{}：{e}", path.display()))?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("NFO 超过 4 MB".into());
    }
    parse(std::str::from_utf8(&bytes).map_err(|_| "NFO 不是 UTF-8 编码")?)
}
pub fn one(path: &Path, tag: &str) -> Result<Node> {
    let mut roots = read(path)?;
    if roots.len() != 1 || roots[0].name != tag {
        return Err(format!("{} 应为 <{tag}> 格式", path.display()));
    }
    Ok(roots.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xml_entities_cdata_and_actor_ids_are_scoped_correctly() {
        let roots=parse("\u{feff}<?xml version=\"1.0\"?><movie><title>A &amp; B &#x4E2D;</title><plot><![CDATA[<hello> & 世界]]></plot><actor><tmdbid>99</tmdbid></actor><tmdbid>1</tmdbid><uniqueid default='true' type='tmdb'>1</uniqueid><thumb aspect='poster'/></movie>").unwrap();
        assert_eq!(roots[0].value("title"), "A & B 中");
        assert_eq!(roots[0].value("plot"), "<hello> & 世界");
        assert_eq!(roots[0].ids().unwrap().get("tmdb").unwrap(), "1");
        assert!(parse("<movie><title>broken</movie>").is_err());
        assert!(
            parse("<!DOCTYPE movie [<!ENTITY x SYSTEM 'file:///secret'>]><movie>&x;</movie>")
                .is_err()
        );
    }
}
