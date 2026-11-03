//! Collect what yt-dlp wrote beside the media: keep one creator-caption
//! track, read chapters and `SponsorBlock` marks out of the info json,
//! and remove the sidecars the bundle does not keep.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::bundle::{primary_language, CaptionTrack, Chapter, PlatformPriors, SkipRange};

const INFO_JSON: &str = "source.info.json";
const CAPTION_FORMATS: [&str; 2] = ["json3", "vtt"];

/// Shape of `--write-info-json` output as written *before*
/// post-processing: the creator's `chapters` and, when `SponsorBlock`
/// ran, the raw `sponsorblock_chapters` beside them. A merged list
/// (sponsor entries carrying `category`) is read the same way.
#[derive(Deserialize, Default)]
struct InfoJson {
    language: Option<String>,
    #[serde(default)]
    chapters: Option<Vec<RawChapter>>,
    #[serde(default)]
    sponsorblock_chapters: Option<Vec<RawChapter>>,
}

#[derive(Deserialize)]
struct RawChapter {
    start_time: f64,
    end_time: Option<f64>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    categories: Option<Vec<String>>,
}

pub fn collect(dest_dir: &Path) -> Result<PlatformPriors> {
    let info = take_info_json(dest_dir)?;
    let spoken = info.as_ref().and_then(|i| i.language.as_deref());
    let captions = keep_one_caption_track(dest_dir, spoken)?;
    let (chapters, skips) = info.map(split_chapters).unwrap_or_default();
    Ok(PlatformPriors {
        captions,
        chapters,
        skips,
    })
}

/// Parse and delete the info json: it is large, lists every format's
/// signed URL, and everything the bundle needs from it goes in the
/// manifest.
fn take_info_json(dest_dir: &Path) -> Result<Option<InfoJson>> {
    let path = dest_dir.join(INFO_JSON);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)?;
    let info: InfoJson = serde_json::from_str(&raw).context("yt-dlp info json is not valid")?;
    let _ = fs::remove_file(&path);
    Ok(Some(info))
}

fn split_chapters(info: InfoJson) -> (Vec<Chapter>, Vec<SkipRange>) {
    let mut chapters = Vec::new();
    let mut skips = Vec::new();
    let all = info
        .chapters
        .unwrap_or_default()
        .into_iter()
        .chain(info.sponsorblock_chapters.unwrap_or_default());
    for raw in all {
        let Some(end) = raw.end_time.filter(|end| *end > raw.start_time) else {
            continue;
        };
        let category = raw
            .category
            .or_else(|| raw.categories.and_then(|c| c.into_iter().next()));
        match category {
            Some(category) => skips.push(SkipRange {
                start_secs: raw.start_time,
                end_secs: end,
                category,
            }),
            None => chapters.push(Chapter {
                start_secs: raw.start_time,
                end_secs: end,
                title: raw.title.unwrap_or_default(),
            }),
        }
    }
    chapters.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    skips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    skips.dedup();
    (chapters, skips)
}

/// Every creator track was downloaded because the spoken language is not
/// known until the info json is read. Keep the track in that language
/// (else the only one, else English, else the first), drop the rest.
fn keep_one_caption_track(dest_dir: &Path, spoken: Option<&str>) -> Result<Option<CaptionTrack>> {
    let mut tracks: Vec<(String, String)> = fs::read_dir(dest_dir)?
        .flatten()
        .filter_map(|entry| caption_file(&entry.file_name().to_string_lossy()))
        .collect();
    tracks.sort();
    let chosen = choose(&tracks, spoken).cloned();
    for track in &tracks {
        if Some(track) != chosen.as_ref() {
            let _ = fs::remove_file(dest_dir.join(file_name(track)));
        }
    }
    let mut languages: Vec<String> = tracks.iter().map(|(lang, _)| lang.clone()).collect();
    languages.dedup();
    Ok(chosen.map(|track| CaptionTrack {
        path: file_name(&track),
        language: track.0,
        format: track.1,
        available_languages: languages,
    }))
}

/// `source.<lang>.<json3|vtt>` → (lang, format); anything else is not a
/// caption file (`source.mp4`, `source.info.json`).
fn caption_file(name: &str) -> Option<(String, String)> {
    let (lang, ext) = name.strip_prefix("source.")?.rsplit_once('.')?;
    (CAPTION_FORMATS.contains(&ext) && !lang.is_empty() && !lang.contains('.'))
        .then(|| (lang.to_string(), ext.to_string()))
}

fn file_name((lang, ext): &(String, String)) -> String {
    format!("source.{lang}.{ext}")
}

/// Tracks are sorted by language then format, so json3 wins over vtt
/// for the same language.
fn choose<'a>(
    tracks: &'a [(String, String)],
    spoken: Option<&str>,
) -> Option<&'a (String, String)> {
    let find = |pred: &dyn Fn(&str) -> bool| tracks.iter().find(|(lang, _)| pred(lang));
    let spoken_primary = spoken.map(primary_language);
    let single_language = tracks
        .iter()
        .all(|(lang, _)| tracks.first().is_some_and(|(first, _)| first == lang));
    find(&|lang| spoken.is_some_and(|s| s.eq_ignore_ascii_case(lang)))
        .or_else(|| {
            find(&|lang| {
                spoken_primary.is_some_and(|s| s.eq_ignore_ascii_case(primary_language(lang)))
            })
        })
        .or_else(|| single_language.then(|| tracks.first()).flatten())
        .or_else(|| find(&|lang| primary_language(lang).eq_ignore_ascii_case("en")))
        .or_else(|| tracks.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(lang: &str, ext: &str) -> (String, String) {
        (lang.to_string(), ext.to_string())
    }

    #[test]
    fn caption_files_are_told_apart_from_the_other_sidecars() {
        assert_eq!(caption_file("source.en.json3"), Some(track("en", "json3")));
        assert_eq!(
            caption_file("source.pt-BR.vtt"),
            Some(track("pt-BR", "vtt"))
        );
        assert_eq!(caption_file("source.info.json"), None);
        assert_eq!(caption_file("source.mp4"), None);
        assert_eq!(caption_file("source.live_chat.json"), None);
        assert_eq!(caption_file("title.txt"), None);
    }

    #[test]
    fn the_spoken_language_wins_then_the_only_one_then_english() {
        let tracks = [
            track("de", "vtt"),
            track("en", "json3"),
            track("pt-BR", "json3"),
        ];
        assert_eq!(choose(&tracks, Some("pt")), Some(&tracks[2]));
        assert_eq!(choose(&tracks, Some("fr")), Some(&tracks[1]));
        assert_eq!(choose(&tracks, None), Some(&tracks[1]));
        let only = [track("ja", "json3"), track("ja", "vtt")];
        assert_eq!(choose(&only, Some("en")), Some(&only[0]));
        let no_english = [track("de", "vtt"), track("ja", "json3")];
        assert_eq!(choose(&no_english, None), Some(&no_english[0]));
        assert_eq!(choose(&[], Some("en")), None);
    }

    #[test]
    fn a_merged_chapter_list_still_separates_sponsor_entries() {
        let info: InfoJson = serde_json::from_str(
            r#"{"chapters":[
                {"start_time":0,"end_time":10,"title":"Intro"},
                {"start_time":10,"end_time":20,"title":"[SponsorBlock]: Sponsor","category":"sponsor","categories":["sponsor"]},
                {"start_time":20,"end_time":null,"title":"Dangling"}
            ]}"#,
        )
        .unwrap();

        let (chapters, skips) = split_chapters(info);

        assert_eq!(chapters.len(), 1, "an entry without an end is dropped");
        assert_eq!(chapters[0].title, "Intro");
        assert_eq!(skips.len(), 1);
        assert_eq!(skips[0].category, "sponsor");
        assert_eq!((skips[0].start_secs, skips[0].end_secs), (10.0, 20.0));
    }
}
