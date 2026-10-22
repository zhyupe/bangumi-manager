use crate::{model::*, paths};
use std::path::{Path, PathBuf};

pub fn root(s: &Settings, kind: &str) -> Result<PathBuf> {
    if !s.output_root.is_empty() {
        Ok(paths::normalize(&s.output_root)?.join(match kind {
            "movie" => "电影",
            "music" => "音乐",
            _ => "电视剧",
        }))
    } else {
        paths::normalize(match kind {
            "movie" => &s.movie_root,
            "music" => &s.music_root,
            _ => &s.tv_root,
        })
    }
}
pub fn category(w: &Work) -> String {
    if !w.category.trim().is_empty() {
        return paths::clean_name(&w.category);
    }
    let chinese = w
        .info
        .countries
        .iter()
        .any(|c| ["CN", "TW", "HK"].contains(&c.as_str()));
    let japan = w.info.countries.iter().any(|c| c == "JP") || w.info.original_language == "ja";
    let anime = w.info.genres.contains(&16);
    match w.kind.as_str() {
        "music" => "专辑",
        "movie" if anime => "动画电影",
        "movie" if chinese => "华语电影",
        "movie" => "外语电影",
        _ if anime && japan => "日番",
        _ if anime && chinese => "国漫",
        _ if anime => "动画",
        _ if w.info.genres.contains(&99) => "纪录片",
        _ if w.info.genres.contains(&10762) => "儿童",
        _ if w.info.genres.contains(&10764) || w.info.genres.contains(&10767) => "综艺",
        _ if chinese => "国产剧",
        _ if japan || w.info.countries.iter().any(|c| c == "KR") => "日韩剧",
        _ if w
            .info
            .countries
            .iter()
            .any(|c| ["US", "GB", "FR", "DE", "CA"].contains(&c.as_str())) =>
        {
            "欧美剧"
        }
        _ => "未分类",
    }
    .into()
}
fn value(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else {
        paths::clean_name(s)
    }
}
pub fn render(
    s: &Settings,
    w: &Work,
    eps: &[&Episode],
    source: &Path,
    probe: Option<&Probe>,
) -> Result<PathBuf> {
    let template = match w.kind.as_str() {
        "movie" => &s.naming.movie,
        "music" => &s.naming.music,
        _ => &s.naming.tv,
    };
    if template.len() > 8192 {
        return Err("命名模板过长".into());
    }
    let ep = eps.first().ok_or("缺少集数映射")?;
    let mut se = format!("S{:02}E{:02}", ep.season, ep.number);
    if eps.len() > 1 {
        se += &format!("-E{:02}", eps.last().unwrap().number);
    }
    let extension = format!(".{}", paths::ext(source));
    let mut env = minijinja::Environment::new();
    env.set_fuel(Some(20_000));
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
    env.add_template("path", template)
        .map_err(|e| format!("命名模板：{e}"))?;
    let rendered = env.get_template("path").unwrap().render(minijinja::context! {
        title => value(&w.title), original_title => value(&w.info.original_title), localized_title => value(&w.info.localized_title),
        year => value(&w.year), tmdbid => &w.tmdb_id, category => category(w), artist => value(&w.info.artist),
        season => format!("{:02}", ep.season), episode => format!("{:02}", ep.number), season_number => ep.season,
        episode_number => ep.number, season_episode => &se, episode_title => value(&ep.title), original_episode_title => value(&ep.info.original_title),
        fileExt => &extension, videoFormat => probe.and_then(|p| p.height).map(|h| format!("{h}p")).unwrap_or_default(),
        videoCodec => value(&probe.and_then(|p| p.video_codec.clone()).unwrap_or_default()), audioCodec => value(&probe.map(|p| p.audio.iter().map(|s| s.split('/').next().unwrap_or("").trim()).collect::<Vec<_>>().join(" ")).unwrap_or_default()),
        releaseGroup => "", part => "", resourceType => "", edition => "", videoEffect => "",
    }).map_err(|e| format!("命名模板：{e}"))?;
    if rendered.len() > 4096 {
        return Err("模板生成的路径过长".into());
    }
    let rendered = rendered.replace('\\', "/");
    let parts: Vec<_> = rendered.split('/').collect();
    if parts.len() < 2
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || p.contains(':')
                || p.chars().any(char::is_control)
        })
    {
        return Err("模板必须生成「作品目录/文件名」，不能包含绝对路径、空目录或 ..".into());
    }
    let path: PathBuf = parts.iter().map(|p| paths::clean_name(p)).collect();
    if paths::ext(&path) != paths::ext(source) {
        return Err("模板必须保留 {{fileExt}} 扩展名".into());
    }
    if w.kind == "tv" && !path.file_name().unwrap().to_string_lossy().contains(&se) {
        return Err("剧集文件名必须包含 {{season_episode}}，以保留季集编号".into());
    }
    if w.kind == "music"
        && !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("{:02}", ep.number))
    {
        return Err("音轨文件名必须以 {{episode}} 开始".into());
    }
    Ok(path)
}
pub fn target(
    s: &Settings,
    w: &Work,
    eps: &[&Episode],
    source: &Path,
    probe: Option<&Probe>,
) -> Result<(PathBuf, PathBuf)> {
    let relative = render(s, w, eps, source, probe)?;
    let base = root(s, &w.kind)?.join(category(w));
    let folder = base.join(relative.components().next().ok_or("缺少作品目录")?);
    Ok((base.join(relative), folder))
}
pub fn previews(s: &Settings) -> Result<serde_json::Value> {
    let ep = Episode {
        title: "始まり".into(),
        info: MediaInfo {
            original_title: "始まり".into(),
            ..Default::default()
        },
        season: 1,
        number: 2,
        ..Default::default()
    };
    let mut result = serde_json::Map::new();
    for kind in ["movie", "tv", "music"] {
        let w = Work {
            title: "作品名称".into(),
            kind: kind.into(),
            year: "2026".into(),
            info: MediaInfo {
                original_title: "作品の名前".into(),
                localized_title: "作品名称".into(),
                artist: "艺术家".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let relative = render(
            s,
            &w,
            &[&ep],
            Path::new(if kind == "music" {
                "source.flac"
            } else {
                "source.mkv"
            }),
            None,
        )?;
        result.insert(
            kind.into(),
            format!(
                "{}/{}/{}",
                match kind {
                    "movie" => "电影",
                    "music" => "音乐",
                    _ => "电视剧",
                },
                category(&w),
                paths::display(&relative)
            )
            .into(),
        );
    }
    Ok(result.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_and_escape_rejection() {
        let mut s = Settings::default();
        let w = Work {
            title: "A/B".into(),
            kind: "tv".into(),
            year: "2020".into(),
            ..Default::default()
        };
        let e = Episode {
            season: 2,
            number: 3,
            ..Default::default()
        };
        let p = render(&s, &w, &[&e], Path::new("x.mkv"), None).unwrap();
        assert!(paths::display(&p).contains("S02E03"));
        assert!(paths::display(&p).starts_with("A_B (2020)"));
        for bad in [
            "../{{season_episode}}{{fileExt}}",
            "C:/{{season_episode}}{{fileExt}}",
            "{{title}}/no-number{{fileExt}}",
            "{{title}}/{{unknown}}",
        ] {
            s.naming.tv = bad.into();
            assert!(render(&s, &w, &[&e], Path::new("x.mkv"), None).is_err());
        }
    }
}
