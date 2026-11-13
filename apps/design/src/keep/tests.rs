#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use super::*;
use kernel::common::FieldName;
use kernel::{Field, FieldSource, FlussoType};

const USERS: &str = include_str!("../../../../dev/users.schema.yml");
const ORDERS: &str = include_str!("../../../../dev/orders.schema.yml");
const CONFIG: &str = include_str!("../../../../dev/flusso.toml");
const RECIPES: &str = include_str!("../../../../libs/2-script/tests/recipes/recipes.schema.yml");

fn parse(text: &str) -> IndexSchema {
    parse_schema(text).expect("schema parses")
}

/// Save `edit` applied to the schema in `base`; the result must keep the
/// layout and read back as the edited schema.
fn save(base: &str, edit: impl FnOnce(&mut IndexSchema)) -> String {
    let mut schema = parse(base);
    edit(&mut schema);
    let saved = schema_yaml(Some(base), &schema, None).unwrap();
    assert_eq!(saved.notice, None, "fell back:\n{}", saved.text);
    assert!(reads_as(parse_schema(&saved.text).as_ref(), &schema));
    saved.text
}

/// The lines only `a` has and the lines only `b` has, by a longest common
/// subsequence of lines.
fn changed(a: &str, b: &str) -> (Vec<String>, Vec<String>) {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            added.push(b[j].to_owned());
            j += 1;
        } else {
            removed.push(a[i].to_owned());
            i += 1;
        }
    }
    (removed, added)
}

fn field<'a>(fields: &'a mut [Field], name: &str) -> &'a mut Field {
    fields
        .iter_mut()
        .find(|f| f.field.as_ref() == name)
        .unwrap_or_else(|| panic!("no field {name}"))
}

fn children<'a>(fields: &'a mut [Field], name: &str) -> &'a mut Vec<Field> {
    match &mut field(fields, name).source {
        FieldSource::Relation(kernel::Relation::Join(join)) => &mut join.fields,
        FieldSource::Group(fields) => fields,
        _ => panic!("{name} has no children"),
    }
}

fn column(field: &mut Field) -> &mut kernel::Column {
    match &mut field.source {
        FieldSource::Column(column) => column,
        _ => panic!("not a column"),
    }
}

fn rename(field: &mut Field, to: &str) {
    field.field = FieldName::try_new(to).unwrap();
}

fn kids(field: &Field) -> &[Field] {
    match &field.source {
        FieldSource::Relation(kernel::Relation::Join(join)) => &join.fields,
        FieldSource::Group(fields) => fields,
        _ => &[],
    }
}

/// The designer's ids for `fields`: each field's path of names, taking the
/// old name of a field `renamed` lists (new, old), so a field keeps its id
/// through a rename and a move, and an added field gets a new one.
fn ids(fields: &[Field], parent: &str, renamed: &[(&str, &str)]) -> Vec<FieldId> {
    fields
        .iter()
        .map(|field| {
            let name = field.field.as_ref();
            let was = renamed
                .iter()
                .find(|(new, _)| *new == name)
                .map_or(name, |(_, old)| *old);
            let uid = format!("{parent}/{was}");
            FieldId {
                name: name.to_owned(),
                fields: ids(kids(field), &uid, renamed),
                uid,
            }
        })
        .collect()
}

/// Save `edit` applied to the schema in `base`, telling the patcher which
/// field is which (`renamed` lists the renames as (new, old)).
fn save_as(base: &str, renamed: &[(&str, &str)], edit: impl FnOnce(&mut IndexSchema)) -> Saved {
    let mut schema = parse(base);
    let before = ids(&schema.fields, "", &[]);
    edit(&mut schema);
    let identity = SchemaIdentity {
        before,
        after: ids(&schema.fields, "", renamed),
    };
    let saved = schema_yaml(Some(base), &schema, Some(&identity)).unwrap();
    assert_eq!(saved.notice, None, "fell back:\n{}", saved.text);
    assert!(reads_as(parse_schema(&saved.text).as_ref(), &schema));
    saved
}

fn remove(fields: &mut Vec<Field>, name: &str) {
    let before = fields.len();
    fields.retain(|f| f.field.as_ref() != name);
    assert_eq!(fields.len() + 1, before, "no field {name}");
}

const ABC: &str = "version: 1\ntable: t\nprimary_key: a\nfields:\n  # about a\n  - integer: a # a\n    required: true\n  # about b\n  - keyword: b # b\n    required: false\n  # about c\n  - keyword: c # c\n    required: false\n";

#[test]
fn an_unchanged_schema_keeps_the_file_byte_for_byte() {
    for base in [USERS, ORDERS] {
        assert_eq!(save(base, |_| {}), base);
    }
}

#[test]
fn a_one_field_edit_to_users_changes_only_that_line() {
    let text = save(USERS, |s| {
        column(field(&mut s.fields, "email")).nullable = true
    });
    assert_eq!(
        changed(USERS, &text),
        (
            vec!["    required: true".into()],
            vec!["    required: false".into()]
        ),
    );
}

#[test]
fn a_one_field_edit_to_orders_changes_only_that_line() {
    let text = save(ORDERS, |s| {
        let items = children(&mut s.fields, "items");
        column(field(items, "quantity")).ty = FlussoType::Long;
    });
    let (removed, added) = changed(ORDERS, &text);
    assert_eq!(removed.len(), 1, "{text}");
    assert_eq!(added.len(), 1, "{text}");
    assert!(removed[0].contains("integer: quantity") && added[0].contains("long: quantity"));
}

#[test]
fn a_header_comment_stays_with_the_file() {
    let text = save(USERS, |s| s.fields.retain(|f| f.field.as_ref() != "id"));
    assert!(text.starts_with(
        "# yaml-language-server: $schema=../libs/1-config/index.schema.yml\n# The `users` search document"
    ));
}

#[test]
fn a_trailing_comment_follows_a_rename() {
    let text = save_as(USERS, &[("displayName", "fullName")], |s| {
        let full_name = field(&mut s.fields, "fullName");
        rename(full_name, "displayName");
    })
    .text;
    assert_eq!(
        changed(USERS, &text),
        (
            vec!["  - text: fullName # natural-language full text".into()],
            vec!["  - text: displayName # natural-language full text".into()],
        ),
    );
}

#[test]
fn a_comment_above_a_field_follows_its_rename_and_type_change() {
    let text = save_as(USERS, &[("membership", "account")], |s| {
        let account = field(&mut s.fields, "account");
        rename(account, "membership");
    })
    .text;
    assert!(
        text.contains(
            "  # A group: reshape same-row scalar columns into a nested object.\n  - object: membership\n"
        ),
        "{text}"
    );
    let text = save(USERS, |s| {
        let tier = field(children(&mut s.fields, "account"), "tier");
        column(tier).ty = FlussoType::Keyword;
    });
    assert_eq!(
        changed(USERS, &text),
        (
            vec!["      - enum: tier # stored as text, indexed as keyword".into()],
            vec!["      - keyword: tier # stored as text, indexed as keyword".into()],
        ),
    );
}

#[test]
fn comments_follow_a_field_that_moves() {
    let text = save(USERS, |s| {
        let at = s
            .fields
            .iter()
            .position(|f| f.field.as_ref() == "orders")
            .unwrap();
        let orders = s.fields.remove(at);
        s.fields.insert(0, orders);
    });
    let orders = text.find("  # One-to-many: orders").unwrap();
    assert!(orders < text.find("  - integer: id").unwrap(), "{text}");
    assert!(
        text.contains(
            "      # Same lifecycle order as the standalone `orders` index, so sorting a\n"
        )
    );
    let (removed, added) = changed(USERS, &text);
    assert_eq!(removed.len(), added.len(), "{text}");
    assert!(
        removed.iter().all(|l| added.contains(l)),
        "only moved lines"
    );
}

#[test]
fn a_deleted_field_takes_its_comments() {
    let text = save(USERS, |s| {
        let account = children(&mut s.fields, "account");
        account.retain(|f| f.field.as_ref() != "tier");
    });
    assert!(!text.contains("stored as text"), "{text}");
    assert_eq!(changed(USERS, &text).1, Vec::<String>::new());
}

#[test]
fn a_deleted_join_takes_its_subtree_and_comments() {
    let text = save(USERS, |s| s.fields.retain(|f| f.field.as_ref() != "orders"));
    assert!(!text.contains("One-to-many: orders"), "{text}");
    assert!(!text.contains("Same lifecycle order"), "{text}");
    assert!(!text.contains("order_items"), "{text}");
    assert!(text.contains("  # Aggregates over the user's orders.\n  - count: orderCount"));
    assert_eq!(changed(USERS, &text).1, Vec::<String>::new());
}

#[test]
fn a_new_field_is_written_in_the_canonical_form_where_it_goes() {
    let text = save(USERS, |s| {
        let mut extra = s.fields[1].clone();
        rename(&mut extra, "altEmail");
        column(&mut extra).column = kernel::common::ColumnName::try_new("alt_email").unwrap();
        s.fields.insert(2, extra);
    });
    let (removed, added) = changed(USERS, &text);
    assert!(removed.is_empty());
    assert_eq!(
        added,
        [
            "  - keyword: altEmail",
            "    column: alt_email",
            "    required: true",
            "    transforms:",
            "    - lowercase",
            "    - trim",
        ]
    );
}

#[test]
fn explicit_defaults_stay_and_omitted_ones_stay_out() {
    let base = "version: 1\ntable: t\nprimary_key: id\nfields:\n- integer: id\n  column: id\n  required: false\n- keyword: k\n  required: true\n";
    let text = save(base, |s| rename(&mut s.fields[1], "kk"));
    assert_eq!(
        text,
        base.replace("keyword: k\n", "keyword: kk\n  column: k\n")
    );
    let base = "version: 1\ntable: t\nprimary_key: id\nfields:\n- integer: id\n  required: true\n- has_one: p\n  table: p\n  foreign_key: t_id\n  primary_key: id\n  fields:\n  - text: bio\n    required: false\n";
    let text = save(base, |s| rename(&mut s.fields[0], "key"));
    assert_eq!(
        text,
        base.replace("integer: id\n", "integer: key\n  column: id\n")
    );
}

#[test]
fn blank_lines_between_fields_survive() {
    let base = "version: 1\ntable: t\nprimary_key: id\n\nfields:\n  - integer: id\n    required: true\n\n  # b\n  - keyword: b\n    required: true\n\n  - keyword: c\n    required: true\n";
    let text = save(base, |s| s.fields.swap(1, 2));
    assert_eq!(
        text,
        "version: 1\ntable: t\nprimary_key: id\n\nfields:\n  - integer: id\n    required: true\n\n  - keyword: c\n    required: true\n\n  # b\n  - keyword: b\n    required: true\n"
    );
}

#[test]
fn a_layout_the_patcher_does_not_follow_falls_back_with_a_notice() {
    let base =
        "{version: 1, table: t, primary_key: id, fields: [{integer: id, required: true}]} # flow\n";
    let mut schema = parse(base);
    rename(&mut schema.fields[0], "key");
    let saved = schema_yaml(Some(base), &schema, None).unwrap();
    assert_eq!(saved.notice, Some(Notice::LayoutReset));
    assert_eq!(saved.text, codegen::schema_to_yaml(&schema).unwrap());
}

#[test]
fn a_new_file_is_the_canonical_form() {
    let schema = parse(USERS);
    let saved = schema_yaml(None, &schema, None).unwrap();
    assert_eq!(saved.notice, None);
    assert_eq!(saved.text, codegen::schema_to_yaml(&schema).unwrap());
}

fn save_config(base: &str, edit: impl FnOnce(&mut ConfigToml)) -> String {
    save_config_as(base, None, edit).text
}

/// Save `edit` applied to the config in `base`; `after` is, when given, the
/// old position of every `[[index]]` entry after the edit.
fn save_config_as(
    base: &str,
    after: Option<Vec<Option<usize>>>,
    edit: impl FnOnce(&mut ConfigToml),
) -> Saved {
    let mut config = ConfigToml::try_parse(base).unwrap();
    let before = config.index.iter().map(|e| e.name.to_string()).collect();
    edit(&mut config);
    let identity = after.map(|after| ConfigIdentity { before, after });
    let saved = config_toml(Some(base), &config, identity.as_ref()).unwrap();
    assert_eq!(saved.notice, None, "fell back:\n{}", saved.text);
    assert!(reads_as(
        ConfigToml::try_parse(&saved.text).ok().as_ref(),
        &config
    ));
    saved
}

#[test]
fn an_unchanged_config_keeps_the_file_byte_for_byte() {
    assert_eq!(save_config(CONFIG, |_| {}), CONFIG);
}

#[test]
fn a_config_edit_keeps_its_comments() {
    let text = save_config(CONFIG, |c| c.index[1].enabled = false);
    assert_eq!(
        changed(CONFIG, &text),
        (
            vec!["enabled = true".into()],
            vec!["enabled = false".into()]
        ),
    );
    let text = save_config(CONFIG, |c| {
        c.index.remove(0);
    });
    assert!(text.starts_with("# flusso dev config"), "{text}");
    assert!(text.contains("# Index defaults (shown with their defaults)."));
    assert!(!text.contains("name = \"users\""));
    // The comment above the first `[[index]]` was its own and goes with it.
    assert!(!text.contains("# Three indexes"), "{text}");
}

#[test]
fn a_reordered_or_added_index_entry_keeps_the_rest() {
    let text = save_config(CONFIG, |c| c.index.swap(0, 2));
    let orders = text.find("name = \"orders\"").unwrap();
    assert!(orders < text.find("name = \"users\"").unwrap(), "{text}");
    let text = save_config(CONFIG, |c| {
        let mut extra = c.index[0].clone();
        extra.name = kernel::common::IndexName::try_new("people").unwrap();
        c.index.push(extra);
    });
    let (removed, added) = changed(CONFIG, &text);
    assert!(removed.is_empty(), "{removed:?}");
    assert!(added.contains(&"name = \"people\"".to_owned()), "{added:?}");
}

// Finding 1: a new item is paired by identity, never by slot.

#[test]
fn a_deleted_join_does_not_lend_its_comments_to_a_renamed_neighbour() {
    let saved = save_as(USERS, &[("ordersTotal", "orderCount")], |s| {
        remove(&mut s.fields, "orders");
        rename(field(&mut s.fields, "orderCount"), "ordersTotal");
    });
    let text = saved.text;
    assert!(!text.contains("One-to-many: orders"), "{text}");
    assert!(
        text.contains("  # Aggregates over the user's orders.\n  - count: ordersTotal\n"),
        "{text}"
    );
    assert_eq!(saved.dropped, 0);
    assert_eq!(changed(USERS, &text).1, ["  - count: ordersTotal"]);
}

#[test]
fn delete_one_and_rename_the_next_keeps_each_comment_with_its_field() {
    let saved = save_as(ABC, &[("cc", "c")], |s| {
        remove(&mut s.fields, "b");
        rename(field(&mut s.fields, "c"), "cc");
    });
    assert_eq!(
        saved.text,
        "version: 1\ntable: t\nprimary_key: a\nfields:\n  # about a\n  - integer: a # a\n    required: true\n  # about c\n  - keyword: cc # c\n    column: c\n    required: false\n"
    );
    assert_eq!(saved.dropped, 0);
}

#[test]
fn a_field_added_where_one_was_deleted_starts_without_comments() {
    let base = "version: 1\ntable: users\nprimary_key: id\nfields:\n  # the primary key\n  - integer: id\n    required: true\n  # SECRET: email is PII, never log it\n  - keyword: email\n    required: false\n";
    let saved = save_as(base, &[], |s| {
        remove(&mut s.fields, "email");
        let mut added = s.fields[0].clone();
        rename(&mut added, "added");
        s.fields.push(added);
    });
    assert!(!saved.text.contains("SECRET"), "{}", saved.text);
    assert!(
        saved.text.ends_with(
            "    required: true\n  - integer: added\n    column: id\n    required: true\n"
        ),
        "{}",
        saved.text
    );
    assert_eq!(saved.dropped, 0);
}

#[test]
fn without_identity_a_rename_is_a_delete_and_an_add_and_its_comments_are_reported() {
    let mut schema = parse(ABC);
    remove(&mut schema.fields, "b");
    rename(field(&mut schema.fields, "c"), "cc");
    let saved = schema_yaml(Some(ABC), &schema, None).unwrap();
    assert!(!saved.text.contains("about b"), "{}", saved.text);
    assert!(!saved.text.contains("about c"), "{}", saved.text);
    assert!(saved.text.contains("  # about a\n  - integer: a # a\n"));
    assert_eq!(saved.dropped, 4, "b and c may still exist: {}", saved.text);
}

#[test]
fn identity_that_does_not_describe_the_file_pairs_by_name() {
    let mut schema = parse(ABC);
    rename(field(&mut schema.fields, "c"), "cc");
    let mut identity = SchemaIdentity {
        before: ids(&parse(ABC).fields, "", &[]),
        after: ids(&schema.fields, "", &[("cc", "c")]),
    };
    identity.before[1].name = "bee".into();
    let saved = schema_yaml(Some(ABC), &schema, Some(&identity)).unwrap();
    assert!(!saved.text.contains("about c"), "{}", saved.text);
    assert!(saved.text.contains("  # about b\n  - keyword: b # b\n"));
    assert_eq!(saved.dropped, 2);
}

#[test]
fn a_removed_index_does_not_lend_its_comments_to_a_renamed_one() {
    let base = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# the users index\n[[index]]\nname = \"users\" # u\nschema = \"users.schema.yml\"\nenabled = true\n\n# the products index\n[[index]]\nname = \"products\" # p\nschema = \"products.schema.yml\"\nenabled = true\n";
    let saved = save_config_as(base, Some(vec![Some(1)]), |c| {
        c.index.remove(0);
        c.index[0].name = kernel::common::IndexName::try_new("goods").unwrap();
    });
    assert_eq!(
        saved.text,
        "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# the products index\n[[index]]\nname = \"goods\" # p\nschema = \"products.schema.yml\"\nenabled = true\n"
    );
    assert_eq!(saved.dropped, 0);
    let saved = save_config_as(CONFIG, Some(vec![Some(0), Some(1)]), |c| {
        c.index.remove(2);
        c.index[1].name = kernel::common::IndexName::try_new("goods").unwrap();
    });
    assert!(saved.text.contains("# Three indexes built from the same store, each a different root table.\n[[index]]\nname = \"users\""));
    assert!(
        saved
            .text
            .contains("name = \"goods\"\nschema = \"products.schema.yml\"")
    );
    assert!(!saved.text.contains("orders"), "{}", saved.text);
}

// Finding 5: a rename and a move in one save.

#[test]
fn a_renamed_field_moved_first_keeps_its_comments() {
    let saved = save_as(ABC, &[("cc", "c")], |s| {
        rename(field(&mut s.fields, "c"), "cc");
        let cc = s.fields.pop().unwrap();
        s.fields.insert(0, cc);
    });
    assert_eq!(
        saved.text,
        "version: 1\ntable: t\nprimary_key: a\nfields:\n  # about c\n  - keyword: cc # c\n    column: c\n    required: false\n  # about a\n  - integer: a # a\n    required: true\n  # about b\n  - keyword: b # b\n    required: false\n"
    );
}

// Finding 2: removing a table keeps the comments the table before it ends with.

#[test]
fn removing_a_sink_keeps_the_trailing_comments_of_the_sink_before_it() {
    let saved = save_config_as(CONFIG, Some(vec![Some(0), Some(1), Some(2)]), |c| {
        c.sinks
            .remove(&kernel::common::SinkName::try_new("audit").unwrap());
    });
    assert!(
        saved.text.contains("tls_verify = false\n# Index defaults (shown with their defaults). text_analysis = \"icu\" needs the\n"),
        "{}",
        saved.text
    );
    assert!(
        saved
            .text
            .contains("# auto_subfields = true\n\n# Three indexes built from the same store"),
        "{}",
        saved.text
    );
    assert!(!saved.text.contains("audit"), "{}", saved.text);
    assert_eq!(changed(CONFIG, &saved.text).1, Vec::<String>::new());
}

#[test]
fn a_tables_own_trailing_comments_go_with_it() {
    let base = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n[sinks.a]\ntype = \"stdout\"\n# a's last word\n\n[sinks.b]\ntype = \"stdout\"\n";
    let text = save_config(base, |c| {
        c.sinks
            .remove(&kernel::common::SinkName::try_new("a").unwrap());
    });
    assert_eq!(
        text,
        "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n[sinks.b]\ntype = \"stdout\"\n"
    );
}

// Finding 3: a comment after a list's last item is the list's.

#[test]
fn a_comment_after_the_last_item_stays_with_the_list() {
    let base = "version: 1\ntable: t\nprimary_key: id\nfields:\n  - object: account\n    fields:\n      - keyword: tier\n        required: false\n      - keyword: plan\n        required: false\n      # - keyword: region\n      #   required: false\n";
    let saved = save_as(base, &[], |s| {
        remove(children(&mut s.fields, "account"), "plan");
    });
    assert_eq!(
        saved.text,
        base.replace("      - keyword: plan\n        required: false\n", "")
    );
}

#[test]
fn deleting_a_recipe_keeps_the_end_anchor_of_the_one_before() {
    let saved = save_as(RECIPES, &[], |s| remove(&mut s.fields, "slug"));
    let text = saved.text;
    assert!(text.contains("  # ANCHOR_END: label\n"), "{text}");
    assert!(!text.contains("# ANCHOR: slug\n"), "{text}");
    assert!(text.contains("  # ANCHOR: display_name\n  - keyword: displayName\n"));
    let (removed, added) = changed(RECIPES, &text);
    assert!(added.is_empty(), "{added:?}");
    assert!(removed.iter().all(|l| !l.contains("ANCHOR_END: label")));
}

// Finding 4: a blank-line-separated comment is not the field's.

#[test]
fn a_heading_separated_by_a_blank_line_stays_when_the_field_goes() {
    let base = "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n\n  # Aggregates over the user's orders.\n\n  - count: orderCount\n    table: orders\n    foreign_key: user_id\n  - count: other\n    table: orders\n    foreign_key: user_id\n";
    let saved = save_as(base, &[], |s| remove(&mut s.fields, "orderCount"));
    assert_eq!(
        saved.text,
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n\n  # Aggregates over the user's orders.\n\n  - count: other\n    table: orders\n    foreign_key: user_id\n"
    );
    // Directly above, with no blank line, it is the field's.
    let saved = save_as(USERS, &[], |s| remove(&mut s.fields, "orderCount"));
    assert!(!saved.text.contains("# Aggregates over"), "{}", saved.text);
}

#[test]
fn a_heading_above_a_group_stays_when_its_first_table_goes() {
    let base = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# ---- sinks ----\n\n[sinks.a]\ntype = \"stdout\"\n\n[sinks.b]\ntype = \"stdout\"\n";
    let text = save_config(base, |c| {
        c.sinks
            .remove(&kernel::common::SinkName::try_new("a").unwrap());
    });
    assert_eq!(
        text,
        "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# ---- sinks ----\n\n[sinks.b]\ntype = \"stdout\"\n"
    );
}

// Finding 6: no silent loss.

#[test]
fn an_item_with_its_dash_alone_or_a_spaced_colon_is_patched_in_place() {
    let base = "version: 1\ntable: users\nprimary_key: id\nfields:\n  -\n    integer: id # pk\n    required: true\n  - # c\n    keyword: email # e\n    required : false\n";
    let saved = save_as(base, &[], |s| {
        column(field(&mut s.fields, "email")).nullable = false
    });
    assert_eq!(
        saved.text,
        base.replace("required : false", "required: true")
    );
    let base = "version: 1\ntable: users\nprimary_key: id\nfields:\n  - integer: id\n    required : true\n  - keyword: email # e\n    # inner comment\n    required : false\n";
    let saved = save_as(base, &[], |s| {
        column(field(&mut s.fields, "email")).nullable = false
    });
    assert_eq!(
        saved.text,
        base.replace("required : false", "required: true")
    );
    assert_eq!(saved.dropped, 0);
}

#[test]
fn emptying_a_join_keeps_the_comments_after_its_list() {
    let base = "version: 1\ntable: users\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - has_one: profile\n    table: profiles\n    foreign_key: user_id\n    primary_key: user_id\n    # the profile fields\n    fields:\n      # about bio\n      - text: bio\n        required: false\n      # trailing inside profile\n    # still profile-ish at join level\n  # about email\n  - keyword: email\n    required: false\n";
    let saved = save_as(base, &[], |s| children(&mut s.fields, "profile").clear());
    assert_eq!(
        saved.text,
        base.replace(
            "    fields:\n      # about bio\n      - text: bio\n        required: false\n",
            "    fields: []\n"
        )
    );
    assert_eq!(saved.dropped, 0);
}

#[test]
fn a_comment_inside_a_rewritten_value_is_reported() {
    let base = "version: 1\ntable: users\nprimary_key: id\nfields:\n  - keyword: email\n    required: true\n    transforms: [\n      lowercase, # first\n      trim,\n    ]\n";
    let saved = save_as(base, &[], |s| {
        column(field(&mut s.fields, "email")).transforms.pop();
    });
    assert!(!saved.text.contains("# first"));
    assert_eq!(saved.dropped, 1);
}

#[test]
fn a_comment_inside_a_rewritten_toml_value_is_reported() {
    let base = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n[sinks.a]\ntype = \"stdout\"\nlist = [\n  1, # one\n  2,\n] # list\n";
    let saved = save_config(base, |c| {
        let sink = c.sinks.values_mut().next().unwrap();
        let toml = toml::to_string(&*sink)
            .unwrap()
            .replace("list = [1, 2]", "list = [2]");
        *sink = toml::from_str(&toml).unwrap();
    });
    assert!(saved.ends_with("list = [2] # list\n"), "{saved}");
    let mut config = ConfigToml::try_parse(base).unwrap();
    let sink = config.sinks.values_mut().next().unwrap();
    let toml = toml::to_string(&*sink)
        .unwrap()
        .replace("list = [1, 2]", "list = [2]");
    *sink = toml::from_str(&toml).unwrap();
    assert_eq!(config_toml(Some(base), &config, None).unwrap().dropped, 1);
}

// Finding 7: the file's header stays first.

#[test]
fn a_new_top_level_key_goes_below_the_header() {
    let saved = save_config_as(CONFIG, None, |c| c.on_error = kernel::FailurePolicy::Skip);
    assert!(
        saved.text.starts_with("# flusso dev config"),
        "{}",
        saved.text
    );
    assert!(
        saved.text.contains("# Schema paths below are resolved relative to this file's directory.\n\non_error = \"skip\"\n\n[source]\n"),
        "{}",
        saved.text
    );
    let (removed, added) = changed(CONFIG, &saved.text);
    assert!(removed.is_empty(), "{removed:?}");
    assert_eq!(
        added
            .into_iter()
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>(),
        ["on_error = \"skip\""]
    );
}

#[test]
fn a_type_change_writes_the_defaults_the_old_type_left_out() {
    let base = "version: 1\ntable: t\nprimary_key: a\nfields:\n  - belongs_to: f0 # f\n    table: t\n    primary_key: a\n    column: a\n    fields:\n      - text: f0\n        column: a\n        required: true\n    emit: false\n";
    let saved = save_as(base, &[], |s| {
        s.fields[0].source = FieldSource::Column(kernel::Column {
            column: kernel::common::ColumnName::try_new("a").unwrap(),
            ty: FlussoType::Text,
            nullable: true,
            transforms: vec![],
            default: None,
            enum_order: vec![],
            subfields: true,
            indexed: true,
        });
    });
    assert_eq!(
        saved.text,
        "version: 1\ntable: t\nprimary_key: a\nfields:\n  - text: f0 # f\n    column: a\n    required: false\n    emit: false\n"
    );
}
