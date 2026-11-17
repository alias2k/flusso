use super::*;
use serde_json::json;

fn read(response: &Value, actions: &[BulkAction]) -> Result<BulkResponse> {
    read_bulk_response(response, &actions.iter().collect::<Vec<_>>())
}

#[test]
fn bulk_body_index_produces_two_ndjson_lines() {
    let doc = json!({ "email": "ada@x.io" }).to_string();
    let actions = vec![BulkAction::Index {
        index: "users".to_owned(),
        id: "42".to_owned(),
        doc,
        require_alias: false,
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
            doc: json!({ "name": "alice" }).to_string(),
            require_alias: false,
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
fn read_bulk_response_is_empty_when_no_errors_flag() {
    let resp = json!({ "errors": false, "items": [] });
    assert!(read(&resp, &[]).unwrap().rejected.is_empty());
}

#[test]
fn read_bulk_response_reports_the_item_with_a_4xx_status_and_its_reason() {
    let resp = json!({
        "errors": true,
        "items": [{ "index": {
            "_index": "users_ab12", "_id": "1", "status": 400,
            "error": { "type": "mapper_parsing_exception", "reason": "failed to parse field" }
        } }]
    });
    let rejected = read(&resp, &[]).unwrap().rejected;
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].index, "users_ab12");
    assert_eq!(rejected[0].id, "1");
    assert_eq!(rejected[0].reason, "failed to parse field");
}

#[test]
fn read_bulk_response_maps_position_to_the_originating_action() {
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
            doc: json!({}).to_string(),
            require_alias: false,
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
    let rejected = read(&resp, &actions).unwrap().rejected;
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].id, "2");
    assert_eq!(rejected[0].reason, "boom");
}

#[test]
fn read_bulk_response_is_empty_when_all_items_succeed() {
    let resp = json!({
        "errors": true,
        "items": [{ "index": { "_index": "x", "_id": "1", "status": 200 } }]
    });
    assert!(read(&resp, &[]).unwrap().rejected.is_empty());
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

#[test]
fn an_alias_targeted_index_action_requires_the_alias() {
    let actions = vec![BulkAction::Index {
        index: "users_ab12".to_owned(),
        id: "1".to_owned(),
        doc: json!({}).to_string(),
        require_alias: true,
    }];
    let body = build_bulk_body(&actions).unwrap();
    let meta: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert_eq!(
        meta,
        json!({ "index": { "_index": "users_ab12", "_id": "1", "require_alias": true } })
    );
}

#[test]
fn a_generation_targeted_index_action_omits_require_alias() {
    let actions = vec![BulkAction::Index {
        index: "users_ab12_2".to_owned(),
        id: "1".to_owned(),
        doc: json!({}).to_string(),
        require_alias: false,
    }];
    let body = build_bulk_body(&actions).unwrap();
    let meta: Value = serde_json::from_str(body.lines().next().unwrap()).unwrap();
    assert!(meta["index"].get("require_alias").is_none());
}

#[test]
fn a_missing_alias_fails_the_whole_flush() {
    let actions = [BulkAction::Index {
        index: "users_ab12".to_owned(),
        id: "1".to_owned(),
        doc: json!({}).to_string(),
        require_alias: true,
    }];
    let resp = json!({
        "errors": true,
        "items": [{ "index": { "_index": "users_ab12", "_id": "1", "status": 404,
            "error": {
                "type": "index_not_found_exception",
                "reason": "no such index [users_ab12] and [require_alias] request flag is [true] and [users_ab12] is not an alias"
            } } }]
    });
    let err = read(&resp, &actions).unwrap_err().to_string();
    assert!(err.contains("users_ab12 is missing"), "{err}");
    assert!(err.contains("is not an alias"), "{err}");
}

#[test]
fn a_delete_against_a_missing_index_fails_the_whole_flush() {
    let actions = [BulkAction::Delete {
        index: "users_ab12".to_owned(),
        id: "1".to_owned(),
    }];
    let resp = json!({
        "errors": true,
        "items": [{ "delete": { "_index": "users_ab12", "_id": "1", "status": 404,
            "error": { "type": "index_not_found_exception", "reason": "no such index [users_ab12]" } } }]
    });
    assert!(read(&resp, &actions).is_err());
}

fn index_action(id: &str) -> BulkAction {
    BulkAction::Index {
        index: "users_1".to_owned(),
        id: id.to_owned(),
        doc: json!({}).to_string(),
        require_alias: false,
    }
}

fn delete_action(id: &str) -> BulkAction {
    BulkAction::Delete {
        index: "users_1".to_owned(),
        id: id.to_owned(),
    }
}

#[test]
fn an_upsert_id_is_refused_past_512_bytes_counted_in_utf8_bytes() {
    for accepted in [
        "a".repeat(512),
        "é".repeat(256),
        format!("{}aa", "é".repeat(255)),
    ] {
        assert_eq!(refused_id(&index_action(&accepted)), None, "{accepted}");
    }
    for refused in [
        "a".repeat(513),
        format!("{}a", "é".repeat(256)),
        "€".repeat(171),
    ] {
        let reason = refused_id(&index_action(&refused)).unwrap();
        assert!(
            reason.contains("513 bytes") && reason.contains("512"),
            "{reason}"
        );
    }
}

#[test]
fn an_empty_id_is_refused_on_both_ops_and_a_long_delete_id_is_not() {
    assert!(refused_id(&index_action("")).unwrap().contains("empty"));
    assert!(refused_id(&delete_action("")).unwrap().contains("empty"));
    assert_eq!(refused_id(&delete_action(&"a".repeat(513))), None);
    assert_eq!(refused_id(&index_action("42")), None);
}

/// The action lines are byte for byte what the bulk body always held: the
/// metadata as `serde_json` renders a `json!` object, keys in that order, every
/// string escaped as JSON escapes it.
#[test]
fn action_lines_are_written_as_before() {
    for (index, id) in [
        ("users", "42"),
        ("odd \"index\"", "a\"b\\c\n\u{1}é\u{2028}"),
        ("", ""),
    ] {
        let delete = bulk_action_fragment(&BulkAction::Delete {
            index: index.to_owned(),
            id: id.to_owned(),
        })
        .unwrap();
        let want =
            serde_json::to_string(&json!({ "delete": { "_index": index, "_id": id } })).unwrap();
        assert_eq!(delete, format!("{want}\n"));
        for require_alias in [false, true] {
            let fragment = bulk_action_fragment(&BulkAction::Index {
                index: index.to_owned(),
                id: id.to_owned(),
                doc: r#"{"a":1}"#.to_owned(),
                require_alias,
            })
            .unwrap();
            let meta = if require_alias {
                json!({ "index": { "_index": index, "_id": id, "require_alias": true } })
            } else {
                json!({ "index": { "_index": index, "_id": id } })
            };
            let meta = serde_json::to_string(&meta).unwrap();
            assert_eq!(fragment, format!("{meta}\n{{\"a\":1}}\n"));
        }
    }
}

fn busy_response(status: u64, kind: &str) -> Value {
    json!({
        "errors": true,
        "items": [
            { "index": { "_index": "users", "_id": "1", "status": 201 } },
            { "index": { "_index": "users", "_id": "2", "status": status,
                         "error": { "type": kind, "reason": "the cluster is busy" } } },
            { "index": { "_index": "users", "_id": "3", "status": 400,
                         "error": { "type": "mapper_parsing_exception",
                                    "reason": "failed to parse field [age]" } } },
        ]
    })
}

#[test]
fn an_item_refused_with_429_is_transient_not_a_rejection() {
    let actions = [index_action("1"), index_action("2"), index_action("3")];
    let response = busy_response(429, "rejected_execution_exception");

    let outcome = read(&response, &actions).unwrap();

    let rejected: Vec<&str> = outcome.rejected.iter().map(|doc| doc.id.as_str()).collect();
    assert_eq!(rejected, ["3"], "only the mapping error is a rejection");
    assert_eq!(outcome.busy, [1], "the busy item is resent by position");
}

#[test]
fn an_item_refused_with_503_is_transient_not_a_rejection() {
    let actions = [index_action("1"), index_action("2"), index_action("3")];
    let response = busy_response(503, "unavailable_shards_exception");

    let outcome = read(&response, &actions).unwrap();

    let rejected: Vec<&str> = outcome.rejected.iter().map(|doc| doc.id.as_str()).collect();
    assert_eq!(rejected, ["3"], "only the mapping error is a rejection");
    assert_eq!(outcome.busy, [1], "the busy item is resent by position");
}

#[test]
fn a_busy_item_no_action_answers_fails_the_flush() {
    let response = busy_response(429, "rejected_execution_exception");

    let error = read(&response, &[index_action("1")])
        .unwrap_err()
        .to_string();

    assert!(error.contains("refused as busy"), "{error}");
}
