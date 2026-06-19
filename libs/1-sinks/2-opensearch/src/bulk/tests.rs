use super::*;

#[test]
fn bulk_body_index_produces_two_ndjson_lines() {
    let doc = json!({ "email": "ada@x.io" });
    let actions = vec![BulkAction::Index {
        index: "users".to_owned(),
        id: "42".to_owned(),
        doc,
    }];
    let body = build_bulk_body(&actions).unwrap();
    let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
    assert_eq!(lines.len(), 2);

    let meta: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(meta["index"]["_index"], "users");
    assert_eq!(meta["index"]["_id"], "42");

    let source: Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(source["email"], "ada@x.io");
}

#[test]
fn bulk_body_delete_produces_one_ndjson_line() {
    let actions = vec![BulkAction::Delete {
        index: "users".to_owned(),
        id: "7".to_owned(),
    }];
    let body = build_bulk_body(&actions).unwrap();
    let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
    assert_eq!(lines.len(), 1);

    let meta: Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(meta["delete"]["_index"], "users");
    assert_eq!(meta["delete"]["_id"], "7");
}

#[test]
fn bulk_body_mixed_operations_are_ordered() {
    let actions = vec![
        BulkAction::Index {
            index: "users".to_owned(),
            id: "1".to_owned(),
            doc: json!({ "name": "alice" }),
        },
        BulkAction::Delete {
            index: "users".to_owned(),
            id: "2".to_owned(),
        },
    ];
    let body = build_bulk_body(&actions).unwrap();
    let lines: Vec<&str> = body.trim_end_matches('\n').split('\n').collect();
    // index: 2 lines, delete: 1 line
    assert_eq!(lines.len(), 3);
    let first_meta: Value = serde_json::from_str(lines[0]).unwrap();
    assert!(first_meta.get("index").is_some());
    let delete_meta: Value = serde_json::from_str(lines[2]).unwrap();
    assert!(delete_meta.get("delete").is_some());
}

#[test]
fn bulk_url_no_pipeline_no_refresh() {
    assert_eq!(
        build_bulk_url("http://localhost:9200", None, false),
        "http://localhost:9200/_bulk"
    );
}

#[test]
fn bulk_url_refresh_only() {
    assert_eq!(
        build_bulk_url("http://localhost:9200", None, true),
        "http://localhost:9200/_bulk?refresh=true"
    );
}

#[test]
fn bulk_url_pipeline_and_refresh() {
    assert_eq!(
        build_bulk_url("http://localhost:9200", Some("my-pipeline"), true),
        "http://localhost:9200/_bulk?pipeline=my-pipeline&refresh=true"
    );
}

#[test]
fn bulk_rejected_is_empty_when_no_errors_flag() {
    let resp = json!({ "errors": false, "items": [] });
    assert!(bulk_rejected(&resp, &[]).is_empty());
}

#[test]
fn bulk_rejected_reports_the_item_with_a_4xx_status_and_its_reason() {
    let resp = json!({
        "errors": true,
        "items": [{ "index": {
            "_index": "users_ab12", "_id": "1", "status": 400,
            "error": { "type": "mapper_parsing_exception", "reason": "failed to parse field" }
        } }]
    });
    let rejected = bulk_rejected(&resp, &[]);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].index, "users_ab12");
    assert_eq!(rejected[0].id, "1");
    assert_eq!(rejected[0].reason, "failed to parse field");
}

#[test]
fn bulk_rejected_maps_position_to_the_originating_action() {
    // Two actions; the second is rejected. The rejection carries the
    // action's index/id (by position), not just the response's echo.
    let actions = [
        BulkAction::Delete {
            index: "users_ab12".to_owned(),
            id: "1".to_owned(),
        },
        BulkAction::Index {
            index: "users_ab12".to_owned(),
            id: "2".to_owned(),
            doc: json!({}),
        },
    ];
    let resp = json!({
        "errors": true,
        "items": [
            { "delete": { "_index": "users_ab12", "_id": "1", "status": 200 } },
            { "index": { "_index": "users_ab12", "_id": "2", "status": 400,
                         "error": { "reason": "boom" } } }
        ]
    });
    let rejected = bulk_rejected(&resp, &actions);
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].id, "2");
    assert_eq!(rejected[0].reason, "boom");
}

#[test]
fn bulk_rejected_is_empty_when_all_items_succeed() {
    let resp = json!({
        "errors": true,
        "items": [{ "index": { "_index": "x", "_id": "1", "status": 200 } }]
    });
    assert!(bulk_rejected(&resp, &[]).is_empty());
}

#[test]
fn build_bulk_body_is_empty_for_no_actions() {
    let body = build_bulk_body(&[]).unwrap();
    assert!(body.is_empty());
}

#[test]
fn plan_chunks_splits_on_the_count_cap() {
    // 5 small actions, cap of 2 per request → 2 + 2 + 1.
    let sizes = [10, 10, 10, 10, 10];
    assert_eq!(plan_chunks(&sizes, 2, 1_000), vec![2, 2, 1]);
}

#[test]
fn plan_chunks_splits_on_the_byte_cap_before_the_count_cap() {
    // Count cap is generous (100), but 30 bytes per request fits only two
    // 12-byte actions; the third would reach 36 > 30, so it starts a new one.
    let sizes = [12, 12, 12, 12];
    assert_eq!(plan_chunks(&sizes, 100, 30), vec![2, 2]);
}

#[test]
fn plan_chunks_isolates_an_oversized_action() {
    // The 50-byte action exceeds the 30-byte cap: it can't be split, so it
    // gets its own request, and the neighbors pack around it.
    let sizes = [10, 50, 10, 10];
    assert_eq!(plan_chunks(&sizes, 100, 30), vec![1, 1, 2]);
}

#[test]
fn plan_chunks_applies_whichever_cap_is_hit_first() {
    // Count cap 3 and byte cap 100: the byte cap bites first at 40+40+40.
    let sizes = [40, 40, 40, 5, 5];
    assert_eq!(plan_chunks(&sizes, 3, 100), vec![2, 3]);
}

#[test]
fn plan_chunks_of_nothing_is_no_requests() {
    assert!(plan_chunks(&[], 10, 100).is_empty());
}
