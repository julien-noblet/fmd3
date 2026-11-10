use crate::models::ChapterItem;
use crate::packer::extract_chapter_number;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChapterGap {
    pub start: f64,
    pub end: f64,
    pub count: usize,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceRef {
    pub module_id: String,
    pub module_name: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergedChapterItem {
    pub chapter_number: Option<f64>,
    pub display_number: String,
    pub title: String,
    pub is_gap_fill: bool,
    pub selected_source: SourceRef,
    pub available_sources: Vec<SourceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergedMangaResult {
    pub total_chapters: usize,
    pub primary_count: usize,
    pub filled_count: usize,
    pub gaps_detected: Vec<ChapterGap>,
    pub chapters: Vec<MergedChapterItem>,
}

/// Parse floating point numerical value of a chapter title (e.g. "Chapitre 1180.5" -> Some(1180.5))
pub fn parse_chapter_number_value(title: &str) -> Option<f64> {
    extract_chapter_number(title).and_then(|s| s.parse::<f64>().ok())
}

/// Detect missing intervals in a series of sorted chapter numbers
pub fn detect_chapter_gaps(numbers: &[f64]) -> Vec<ChapterGap> {
    let mut gaps = Vec::new();
    if numbers.len() < 2 {
        return gaps;
    }

    let mut sorted = numbers.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted.dedup_by(|a, b| (*a - *b).abs() < 0.0001);

    for window in sorted.windows(2) {
        let a = window[0];
        let b = window[1];

        // Only check gaps when diff > 1.0 (excluding decimal increments like 1.1, 1.2)
        if b - a > 1.0001 {
            let start = (a.floor() as i64) + 1;
            let end = (b.floor() as i64) - 1;

            if start <= end {
                let count = (end - start + 1) as usize;
                let description = if start == end {
                    format!("Chapitre {} manquant", start)
                } else {
                    format!("Chapitres {} à {} manquants ({} chapitres)", start, end, count)
                };

                gaps.push(ChapterGap {
                    start: start as f64,
                    end: end as f64,
                    count,
                    description,
                });
            }
        }
    }

    gaps
}

fn float_key(val: f64) -> i64 {
    (val * 10000.0).round() as i64
}

/// Merge chapters across multiple sources, filling gaps in primary source using fallback source(s)
pub fn merge_chapter_sources(
    primary_module: (&str, &str),
    primary_chapters: &[ChapterItem],
    fallback_sources: &[((&str, &str), &[ChapterItem])],
) -> MergedMangaResult {
    let mut numbered_map: BTreeMap<i64, MergedChapterItem> = BTreeMap::new();
    let mut unnumbered_primary: Vec<MergedChapterItem> = Vec::new();
    let mut unnumbered_fallback: Vec<MergedChapterItem> = Vec::new();

    let mut primary_nums = Vec::new();

    // 1. Ingest primary source chapters
    for ch in primary_chapters {
        let num_opt = parse_chapter_number_value(&ch.title);
        let src = SourceRef {
            module_id: primary_module.0.to_string(),
            module_name: primary_module.1.to_string(),
            title: ch.title.clone(),
            url: ch.url.clone(),
        };

        let disp = num_opt
            .map(|n| {
                if n.fract() == 0.0 {
                    format!("{}", n as i64)
                } else {
                    format!("{}", n)
                }
            })
            .unwrap_or_default();

        let item = MergedChapterItem {
            chapter_number: num_opt,
            display_number: disp,
            title: ch.title.clone(),
            is_gap_fill: false,
            selected_source: src.clone(),
            available_sources: vec![src],
        };

        if let Some(num) = num_opt {
            primary_nums.push(num);
            numbered_map.insert(float_key(num), item);
        } else {
            unnumbered_primary.push(item);
        }
    }

    let gaps = detect_chapter_gaps(&primary_nums);

    // 2. Ingest fallback sources to fill gaps or offer alternative versions
    for &((fallback_mod_id, fallback_mod_name), secondary_chapters) in fallback_sources {
        for ch in secondary_chapters {
            let num_opt = parse_chapter_number_value(&ch.title);
            let src = SourceRef {
                module_id: fallback_mod_id.to_string(),
                module_name: fallback_mod_name.to_string(),
                title: ch.title.clone(),
                url: ch.url.clone(),
            };

            if let Some(num) = num_opt {
                let key = float_key(num);
                if let Some(existing) = numbered_map.get_mut(&key) {
                    if !existing.available_sources.iter().any(|s| s.module_id == fallback_mod_id && s.url == ch.url) {
                        existing.available_sources.push(src);
                    }
                } else {
                    // Gap fill!
                    let disp = if num.fract() == 0.0 {
                        format!("{}", num as i64)
                    } else {
                        format!("{}", num)
                    };

                    let item = MergedChapterItem {
                        chapter_number: Some(num),
                        display_number: disp,
                        title: ch.title.clone(),
                        is_gap_fill: true,
                        selected_source: src.clone(),
                        available_sources: vec![src],
                    };
                    numbered_map.insert(key, item);
                }
            } else {
                let exists = unnumbered_primary.iter().any(|p| p.title.eq_ignore_ascii_case(&ch.title))
                    || unnumbered_fallback.iter().any(|f| f.title.eq_ignore_ascii_case(&ch.title));

                if !exists {
                    let item = MergedChapterItem {
                        chapter_number: None,
                        display_number: String::new(),
                        title: ch.title.clone(),
                        is_gap_fill: true,
                        selected_source: src.clone(),
                        available_sources: vec![src],
                    };
                    unnumbered_fallback.push(item);
                }
            }
        }
    }

    // 3. Assemble and sort complete list
    let mut merged = Vec::new();
    merged.extend(unnumbered_primary);
    for (_k, item) in numbered_map {
        merged.push(item);
    }
    merged.extend(unnumbered_fallback);

    let total = merged.len();
    let primary_count = merged.iter().filter(|c| !c.is_gap_fill).count();
    let filled_count = merged.iter().filter(|c| c.is_gap_fill).count();

    MergedMangaResult {
        total_chapters: total,
        primary_count,
        filled_count,
        gaps_detected: gaps,
        chapters: merged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_chapter_gaps() {
        // MangaPlus pattern for Jujutsu Kaisen: [1, 2, 3, 268, 269, 270, 271]
        let nums = vec![1.0, 2.0, 3.0, 268.0, 269.0, 270.0, 271.0];
        let gaps = detect_chapter_gaps(&nums);

        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].start, 4.0);
        assert_eq!(gaps[0].end, 267.0);
        assert_eq!(gaps[0].count, 264);
        assert!(gaps[0].description.contains("Chapitres 4 à 267 manquants (264 chapitres)"));
    }

    #[test]
    fn test_merge_chapter_sources_gap_fill() {
        let primary_mod = ("mangaplus", "MangaPlus");
        let primary_chaps = vec![
            ChapterItem { title: "001 Ryomen Sukuna".into(), url: "mp/1".into() },
            ChapterItem { title: "002 Secret Execution".into(), url: "mp/2".into() },
            ChapterItem { title: "003 For Myself".into(), url: "mp/3".into() },
            ChapterItem { title: "270 From Here On".into(), url: "mp/270".into() },
            ChapterItem { title: "271 Final Chapter".into(), url: "mp/271".into() },
        ];

        let secondary_mod = ("sushiscan", "SushiScan");
        let secondary_chaps = vec![
            ChapterItem { title: "Chapitre 1".into(), url: "ss/1".into() },
            ChapterItem { title: "Chapitre 2".into(), url: "ss/2".into() },
            ChapterItem { title: "Chapitre 3".into(), url: "ss/3".into() },
            ChapterItem { title: "Chapitre 4".into(), url: "ss/4".into() },
            ChapterItem { title: "Chapitre 5".into(), url: "ss/5".into() },
            ChapterItem { title: "Chapitre 270".into(), url: "ss/270".into() },
        ];

        let res = merge_chapter_sources(
            primary_mod,
            &primary_chaps,
            &[((secondary_mod.0, secondary_mod.1), &secondary_chaps)],
        );

        assert_eq!(res.total_chapters, 7); // 1, 2, 3, 4, 5, 270, 271
        assert_eq!(res.primary_count, 5);
        assert_eq!(res.filled_count, 2); // 4 and 5 filled
        assert_eq!(res.gaps_detected.len(), 1); // 4 to 269

        // Chapter 1: selected = MangaPlus, available = MangaPlus + SushiScan
        let ch1 = &res.chapters[0];
        assert_eq!(ch1.chapter_number, Some(1.0));
        assert_eq!(ch1.is_gap_fill, false);
        assert_eq!(ch1.selected_source.module_id, "mangaplus");
        assert_eq!(ch1.available_sources.len(), 2);

        // Chapter 4: selected = SushiScan, available = SushiScan, is_gap_fill = true
        let ch4 = &res.chapters[3];
        assert_eq!(ch4.chapter_number, Some(4.0));
        assert_eq!(ch4.is_gap_fill, true);
        assert_eq!(ch4.selected_source.module_id, "sushiscan");
        assert_eq!(ch4.available_sources.len(), 1);

        // Chapter 271: selected = MangaPlus, available = MangaPlus
        let ch271 = &res.chapters[6];
        assert_eq!(ch271.chapter_number, Some(271.0));
        assert_eq!(ch271.is_gap_fill, false);
        assert_eq!(ch271.selected_source.module_id, "mangaplus");
    }
}
