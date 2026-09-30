use multizen_core::*;

// These identifiers are synthesized test strings, not real personal data.
fn synthetic_card(date: &str, sequence: &str) -> String {
    let prefix = format!("990001{date}{sequence}");
    let weights = [7u32, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let sum: u32 = prefix
        .bytes()
        .zip(weights)
        .map(|(c, w)| u32::from(c - b'0') * w)
        .sum();
    format!("{prefix}{}", b"10X98765432"[(sum % 11) as usize] as char)
}

#[test]
fn validates_checksum_dates_structure_and_lowercase_x() {
    let evidence = SubjectVisibleEvidence::default();
    let valid = synthetic_card("20000229", "123");
    let result = validate_subject_fields("合成样本", &valid, &evidence, "2026-10-01");
    assert!(result.can_confirm());
    assert_eq!(result.visible_id_card, ValidationCheck::Unavailable);
    assert_eq!(result.visible_name, ValidationCheck::Unavailable);
    for date in [
        "19000229", "20250229", "20000431", "20000001", "20000100", "20261002", "00000101",
    ] {
        assert_eq!(
            validate_subject_fields(
                "合成样本",
                &synthetic_card(date, "123"),
                &evidence,
                "2026-10-01"
            )
            .birth_date,
            ValidationCheck::Failed
        );
    }
    let mut bad = valid.clone();
    bad.replace_range(17.., if &valid[17..] == "0" { "1" } else { "0" });
    assert_eq!(
        validate_subject_fields("合成样本", &bad, &evidence, "2026-10-01").checksum,
        ValidationCheck::Failed
    );
    for value in [
        "１２３４５",
        "12345",
        "99000120000101000X",
        "00000120000101123X",
        "99000120000101O23X",
    ] {
        assert!(!validate_subject_fields("合成样本", value, &evidence, "2026-10-01").can_confirm());
    }
    let with_x = (1..1000)
        .map(|n| synthetic_card("20000101", &format!("{n:03}")))
        .find(|s| s.ends_with('X'))
        .unwrap();
    assert!(
        validate_subject_fields("合成样本", &with_x.to_lowercase(), &evidence, "2026-10-01")
            .can_confirm()
    );
    assert!(!validate_subject_fields("合*样本", &valid, &evidence, "2026-10-01").can_confirm());
    assert!(!validate_subject_fields("- ·", &valid, &evidence, "2026-10-01").can_confirm());
    assert!(
        !validate_subject_fields("合成样本", &valid, &evidence, "2026-10-01-extra").can_confirm()
    );
    assert!(!validate_subject_fields("合成样本", &valid, &evidence, "not-a-date").can_confirm());
}

#[test]
fn visible_fragments_are_anchored_and_unavailable_is_distinct() {
    use ValidationCheck::*;
    for value in [None, Some(""), Some("****"), Some("＊●•")] {
        assert_eq!(compare_subject_visible("合成样本", value), Unavailable);
    }
    for (pattern, expected) in [
        ("合*本", Passed),
        ("合**样本", Passed),
        ("*样*", Passed),
        ("合成样本", Passed),
        ("错*本", Failed),
        ("*合成", Failed),
        ("合成样本*", Failed),
    ] {
        assert_eq!(compare_subject_visible("合成样本", Some(pattern)), expected);
    }
    let card = synthetic_card("20000101", "123");
    let evidence = SubjectVisibleEvidence {
        real_name: Some("错*".into()),
        id_card: Some("990001************".into()),
    };
    let result = validate_subject_fields("合成样本", &card, &evidence, "2026-10-01");
    assert_eq!(result.visible_id_card, Passed);
    assert_eq!(result.visible_name, Failed);
    assert!(!result.can_confirm());
}

#[test]
fn image_contract_rejects_paths_bad_hash_metadata_and_duplicates() {
    let image = SubjectAttachment {
        key: format!("{}.png", "a".repeat(64)),
        sha256: "a".repeat(64),
        mime_type: "image/png".into(),
        byte_len: 100,
        width: 10,
        height: 10,
    };
    assert!(valid_subject_attachments(std::slice::from_ref(&image)));
    for key in [
        "../a.png",
        "C:\\secret.jpg",
        "https://host/a.png",
        "/a.png",
        "AA.png",
        "abc.png:stream",
    ] {
        assert!(!valid_subject_attachment_key(key));
    }
    assert!(!valid_subject_attachments(&[]));
    assert!(!valid_subject_attachments(&[image.clone(), image.clone()]));
    let mut bad = image.clone();
    bad.width = 0;
    assert!(!bad.is_valid());
    bad = image.clone();
    bad.byte_len = SUBJECT_MAX_IMAGE_BYTES + 1;
    assert!(!bad.is_valid());
    bad = image.clone();
    bad.height = SUBJECT_MAX_IMAGE_DIMENSION + 1;
    assert!(!bad.is_valid());
    bad = image.clone();
    bad.sha256 = "b".repeat(64);
    assert!(!bad.is_valid());
    bad = image;
    bad.mime_type = "image/svg+xml".into();
    assert!(!bad.is_valid());
}

#[test]
fn wire_is_camel_case_strict_and_debug_redacts_fields() {
    let card = synthetic_card("20000101", "123");
    let input = CorrectSubjectInput {
        platform_user_id: "12345".into(),
        expected_revision: 4,
        real_name: "合成样本".into(),
        id_card: card.clone(),
    };
    let value = serde_json::to_value(&input).unwrap();
    assert_eq!(value["expectedRevision"], 4);
    assert_eq!(
        serde_json::to_value(SubjectSource::TalentTabPlaintext).unwrap(),
        "talent-tab-plaintext"
    );
    assert_eq!(
        serde_json::to_value(SubjectReviewStatus::PendingReview).unwrap(),
        "pending-review"
    );
    let mut extra = value;
    extra["reviewStatus"] = "confirmed".into();
    assert!(serde_json::from_value::<CorrectSubjectInput>(extra).is_err());
    assert!(!format!("{input:?}").contains(&card));
    assert!(!format!("{input:?}").contains("合成样本"));
    let query = KuaishouSubjectQuery {
        search: card.clone(),
        offset: 0,
        limit: 10,
    };
    assert!(!format!("{query:?}").contains(&card));
    assert!(!masked_subject_id_card(&card).contains(&card));
}
