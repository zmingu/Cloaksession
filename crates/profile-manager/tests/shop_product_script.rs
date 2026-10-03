use profile_manager::{
    AddShopProductScriptLineInput, CreateShopProductScriptInput, ProfileManager, ScriptLineAction,
    UpdateShopProductScriptInput, UpdateShopProductScriptLineInput,
};
use tempfile::TempDir;

fn manager(dir: &TempDir) -> ProfileManager {
    ProfileManager::new(
        &dir.path().join("profiles.db"),
        &dir.path().join("profiles"),
    )
    .unwrap()
}

fn create_script(pm: &ProfileManager, name: &str) -> profile_manager::ShopProductScript {
    pm.shop_product_script_create(CreateShopProductScriptInput {
        name: name.into(),
        description: None,
    })
    .unwrap()
}

fn add_line(
    pm: &ProfileManager,
    script_id: &str,
    action: ScriptLineAction,
    goods_id: &str,
    video_time_sec: f64,
    lead_sec: f64,
) -> profile_manager::ShopProductScriptLine {
    pm.shop_product_script_add_line(AddShopProductScriptLineInput {
        script_id: script_id.into(),
        action,
        goods_id: goods_id.into(),
        goods_name: None,
        video_time_sec,
        lead_sec,
        content: "讲解词".into(),
        sort_order: None,
    })
    .unwrap()
}

#[test]
fn migration_is_idempotent_and_enforces_checks_and_cascade() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    for _ in 0..3 {
        profile_manager::migrate::run_migrations(&conn).unwrap();
    }
    for table in ["shop_product_scripts", "shop_product_script_lines"] {
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "{table}");
    }
    let idx: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='idx_shop_product_script_lines_script'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(idx, 1);
    // CHECK 约束：非法动作 / 负时间拒绝写入。
    conn.execute_batch(
        "INSERT INTO shop_product_scripts(id,name,created_at,updated_at) VALUES ('s','S','t','t');",
    )
    .unwrap();
    assert!(conn
        .execute(
            "INSERT INTO shop_product_script_lines(id,script_id,action,goods_id,video_time_sec,lead_sec,content,created_at,updated_at) VALUES ('l','s','bad-action','g',1,0,'','t','t')",
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "INSERT INTO shop_product_script_lines(id,script_id,action,goods_id,video_time_sec,lead_sec,content,created_at,updated_at) VALUES ('l','s','explain','g',-1,0,'','t','t')",
            [],
        )
        .is_err());
    assert!(conn
        .execute("INSERT INTO shop_product_script_lines(id,script_id,action,goods_id,video_time_sec,lead_sec,content,created_at,updated_at) VALUES ('l','s','explain','g',1,0,'','t','t')", [])
        .is_ok());
    // 外键级联：删除脚本带走行。
    conn.execute("DELETE FROM shop_product_scripts WHERE id='s'", [])
        .unwrap();
    let remaining: i64 = conn
        .query_row("SELECT count(*) FROM shop_product_script_lines", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn script_crud_round_trip_with_reopen() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    assert!(pm.shop_product_scripts_list().unwrap().is_empty());
    assert!(pm.shop_product_script_get("missing").unwrap().is_none());

    let script = pm
        .shop_product_script_create(CreateShopProductScriptInput {
            name: "  早场话术  ".into(),
            description: Some("  每日开播用  ".into()),
        })
        .unwrap();
    assert_eq!(script.name, "早场话术");
    assert_eq!(script.description.as_deref(), Some("每日开播用"));
    chrono::DateTime::parse_from_rfc3339(&script.created_at).unwrap();

    let updated = pm
        .shop_product_script_update(
            &script.id,
            UpdateShopProductScriptInput {
                name: Some("晚场话术".into()),
                description: Some(None),
            },
        )
        .unwrap();
    assert_eq!(updated.id, script.id);
    assert_eq!(updated.name, "晚场话术");
    assert_eq!(updated.description, None);
    assert_eq!(updated.created_at, script.created_at);

    drop(pm);
    let pm = manager(&dir);
    assert_eq!(pm.shop_product_scripts_list().unwrap().len(), 1);
    let detail = pm.shop_product_script_get(&script.id).unwrap().unwrap();
    assert_eq!(detail.script.name, "晚场话术");
    assert!(detail.lines.is_empty());

    pm.shop_product_script_delete(&script.id).unwrap();
    pm.shop_product_script_delete(&script.id).unwrap();
    assert!(pm.shop_product_scripts_list().unwrap().is_empty());
}

#[test]
fn line_crud_appends_sort_order_and_reorders() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    let script = create_script(&pm, "S");
    let a = add_line(&pm, &script.id, ScriptLineAction::OnShelf, "g1", 10.0, 0.0);
    let b = add_line(&pm, &script.id, ScriptLineAction::Explain, "g2", 60.0, 5.0);
    assert_eq!((a.sort_order, b.sort_order), (0, 1));

    let edited = pm
        .shop_product_script_update_line(
            &b.id,
            UpdateShopProductScriptLineInput {
                goods_name: Some(Some("二号商品".into())),
                lead_sec: Some(3.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(edited.goods_name.as_deref(), Some("二号商品"));
    assert_eq!(edited.lead_sec, 3.0);
    assert_eq!(edited.video_time_sec, 60.0);

    let reordered = pm
        .shop_product_script_reorder_lines(&script.id, vec![b.id.clone(), a.id.clone()])
        .unwrap();
    assert_eq!(reordered[0].id, b.id);
    assert_eq!(reordered[1].id, a.id);
    assert_eq!((reordered[0].sort_order, reordered[1].sort_order), (0, 1));

    pm.shop_product_script_delete_line(&b.id).unwrap();
    let detail = pm.shop_product_script_get(&script.id).unwrap().unwrap();
    assert_eq!(detail.lines.len(), 1);
    assert_eq!(detail.lines[0].id, a.id);

    pm.shop_product_script_delete(&script.id).unwrap();
    assert!(pm.shop_product_script_get(&script.id).unwrap().is_none());
}

#[test]
fn invalid_inputs_and_foreign_rows_are_rejected_atomically() {
    let dir = TempDir::new().unwrap();
    let pm = manager(&dir);
    for name in [" ".into(), "x".repeat(101), "a\nb".into()] {
        assert!(pm
            .shop_product_script_create(CreateShopProductScriptInput {
                name,
                description: None
            })
            .is_err());
    }
    assert!(pm
        .shop_product_script_update(
            "missing",
            UpdateShopProductScriptInput {
                name: Some("S".into()),
                description: None
            }
        )
        .is_err());
    assert!(pm
        .shop_product_script_update_line(
            "missing",
            UpdateShopProductScriptLineInput {
                content: Some("x".into()),
                ..Default::default()
            }
        )
        .is_err());
    assert!(pm
        .shop_product_script_reorder_lines("missing", vec![])
        .is_err());
    let script = create_script(&pm, "S");
    // 非法行入参：空商品ID / 负时间 / 非有限数 / 超长内容。
    for line in [
        AddShopProductScriptLineInput {
            script_id: script.id.clone(),
            action: ScriptLineAction::Explain,
            goods_id: "  ".into(),
            goods_name: None,
            video_time_sec: 1.0,
            lead_sec: 0.0,
            content: String::new(),
            sort_order: None,
        },
        AddShopProductScriptLineInput {
            script_id: script.id.clone(),
            action: ScriptLineAction::Explain,
            goods_id: "g".into(),
            goods_name: None,
            video_time_sec: -1.0,
            lead_sec: 0.0,
            content: String::new(),
            sort_order: None,
        },
        AddShopProductScriptLineInput {
            script_id: script.id.clone(),
            action: ScriptLineAction::Explain,
            goods_id: "g".into(),
            goods_name: None,
            video_time_sec: f64::NAN,
            lead_sec: 0.0,
            content: String::new(),
            sort_order: None,
        },
        AddShopProductScriptLineInput {
            script_id: script.id.clone(),
            action: ScriptLineAction::Explain,
            goods_id: "g".into(),
            goods_name: None,
            video_time_sec: 1.0,
            lead_sec: 0.0,
            content: "x".repeat(2001),
            sort_order: None,
        },
        AddShopProductScriptLineInput {
            script_id: "missing".into(),
            action: ScriptLineAction::Explain,
            goods_id: "g".into(),
            goods_name: None,
            video_time_sec: 1.0,
            lead_sec: 0.0,
            content: String::new(),
            sort_order: None,
        },
    ] {
        assert!(pm.shop_product_script_add_line(line).is_err());
    }
    assert!(pm
        .shop_product_script_get(&script.id)
        .unwrap()
        .unwrap()
        .lines
        .is_empty());

    // 重排必须集合一致：缺失 / 混入他脚本行 / 重复都拒绝。
    let other = create_script(&pm, "Other");
    let l1 = add_line(&pm, &script.id, ScriptLineAction::OnShelf, "g1", 1.0, 0.0);
    let foreign = add_line(&pm, &other.id, ScriptLineAction::OnShelf, "g9", 1.0, 0.0);
    assert!(pm
        .shop_product_script_reorder_lines(&script.id, vec![])
        .is_err());
    assert!(pm
        .shop_product_script_reorder_lines(&script.id, vec![l1.id.clone(), foreign.id.clone()])
        .is_err());
    assert!(pm
        .shop_product_script_reorder_lines(&script.id, vec![l1.id.clone(), l1.id.clone()])
        .is_err());
    let detail = pm.shop_product_script_get(&script.id).unwrap().unwrap();
    assert_eq!(detail.lines.len(), 1);
    assert_eq!(detail.lines[0].sort_order, 0);
}
