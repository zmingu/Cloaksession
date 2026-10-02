use std::fmt;

/// Untrusted OCR suggestions, NOT validated identity information. No automatic
/// substitutions of O/0, I/1, B/8. Only whitespace and ASCII x case normalize.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct OcrCandidates {
    pub names: Vec<String>,
    pub id_numbers: Vec<String>,
}

impl fmt::Debug for OcrCandidates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OcrCandidates")
            .field("name_count", &self.names.len())
            .field("id_number_count", &self.id_numbers.len())
            .finish_non_exhaustive()
    }
}

/// Extract conservative suggestions from each recognized line. Labels may have
/// OCR-inserted spaces. Numbers must be complete 18-character runs; a longer
/// digit run is not truncated into an apparently valid ID. Caller owns checksum,
/// date, visible-mask comparison and mandatory human review using core rules.
pub fn extract_candidates(lines: &[String]) -> OcrCandidates {
    let mut result = OcrCandidates::default();
    // First pass: ID numbers (always safe — strict 18-char format).
    for line in lines {
        let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        for run in compact.split(|c: char| !(c.is_ascii_digit() || c == 'x' || c == 'X')) {
            let bytes = run.as_bytes();
            if bytes.len() == 18
                && bytes[..17].iter().all(u8::is_ascii_digit)
                && (bytes[17].is_ascii_digit() || matches!(bytes[17], b'x' | b'X'))
            {
                push_unique(&mut result.id_numbers, run.to_ascii_uppercase());
            }
        }
    }
    let has_id_number = !result.id_numbers.is_empty();
    // Second pass: labelled names (always accepted if format matches).
    for line in lines {
        let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
        for label in ["经营者姓名", "分销者姓名", "姓名"] {
            if let Some(value) = compact.strip_prefix(label) {
                let value = value.trim_start_matches([':', '：']);
                let end = [
                    "性别",
                    "民族",
                    "出生",
                    "住址",
                    "公民身份",
                    "证件号码",
                    "身份证",
                ]
                .into_iter()
                .filter_map(|label| value.find(label))
                .min()
                .unwrap_or(value.len());
                let name = &value[..end];
                if (2..=32).contains(&name.chars().count())
                    && name.chars().all(|c| matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '·' | '•'))
                {
                    push_unique(&mut result.names, name.to_owned());
                }
                break;
            }
        }
    }
    // Third pass: unlabelled standalone CJK name lines from ID card images.
    // Only when: (a) no labelled name was found, (b) an ID number exists in the
    // same OCR result (so we're looking at an ID card, not a random text), and
    // (c) the line is 2-4 CJK-only chars with no field label. This prevents
    // turning "测试" or "性别男" into a name candidate in non-card contexts.
    if result.names.is_empty() && has_id_number {
        for line in lines {
            let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
            let has_field_label = ["性别", "民族", "出生", "住址", "公民身份", "证件号码", "身份证", "号码"]
                .iter()
                .any(|l| compact.contains(l));
            if !has_field_label
                && (2..=4).contains(&compact.chars().count())
                && compact.chars().all(|c| matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '·' | '•'))
            {
                push_unique(&mut result.names, compact.clone());
            }
        }
    }
    result
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_whitespace_duplicates_and_case() {
        let data = extract_candidates(&[
            "姓 名： 测 试 性别男".into(),
            "经营者姓名 测试".into(),
            "公民身份号码 990101 20000101 001x".into(),
        ]);
        assert_eq!(data.names, ["测试"]);
        assert_eq!(data.id_numbers, ["99010120000101001X"]);
    }

    #[test]
    fn rejects_masked_ambiguous_long_and_unlabelled_values() {
        let data = extract_candidates(&[
            "测试".into(),
            "姓名 测*".into(),
            "9901012000010100IX".into(),
            "199010120000101001X".into(),
            "9901012000****001X".into(),
        ]);
        assert!(data.names.is_empty());
        assert!(data.id_numbers.is_empty());
    }

    #[test]
    fn debug_redacts_content() {
        let data = OcrCandidates {
            names: vec!["秘密名字".into()],
            id_numbers: vec!["敏感数字".into()],
        };
        let debug = format!("{data:?}");
        assert!(!debug.contains("秘密"));
        assert!(!debug.contains("敏感"));
    }

    #[test]
    fn unlabelled_standalone_cjk_name_from_id_image() {
        // Real ID card OCR: name appears as a standalone CJK line with no label.
        // "杜 辉 耀" → compact "杜辉耀" (3 chars, CJK-only, no field label).
        let data = extract_candidates(&[
            "性 别".into(),
            "出 生".into(),
            "住 址".into(),
            "杜 辉 耀".into(),
            "男".into(),
            "民 族 汉".into(),
            "公 民 身 份 号 码".into(),
            "1 4 1 1 0 2 2 0 0 7 0 7 1 6 0 0 3 X".into(),
        ]);
        assert_eq!(data.names, ["杜辉耀"]);
        assert_eq!(data.id_numbers, ["14110220070716003X"]);
    }

    #[test]
    fn rejects_field_label_lines_as_names() {
        // "民族汉" compact is 3 CJK chars but contains "民族" — must not become a name.
        let data = extract_candidates(&["民 族 汉".into()]);
        assert!(data.names.is_empty());
    }
}
