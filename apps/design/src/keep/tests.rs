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
    // Only moved lines; moved first, the field leaves its blank line behind.
    let (removed, added) = changed(USERS, &text);
    let (removed, added): (Vec<_>, Vec<_>) = (
        removed.into_iter().filter(|l| !l.is_empty()).collect(),
        added.into_iter().filter(|l| !l.is_empty()).collect(),
    );
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
    let before = ConfigToml::try_parse(base)
        .unwrap()
        .index
        .iter()
        .map(|e| e.name.to_string())
        .collect();
    let identity = after.map(|after| ConfigIdentity {
        before,
        after,
        sinks: None,
    });
    save_config_with(base, identity, edit)
}

/// Save `edit` applied to the config in `base` with `identity`.
fn save_config_with(
    base: &str,
    identity: Option<ConfigIdentity>,
    edit: impl FnOnce(&mut ConfigToml),
) -> Saved {
    let mut config = ConfigToml::try_parse(base).unwrap();
    edit(&mut config);
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
fn a_field_identity_with_untrimmed_names_still_pairs() {
    let mut schema = parse(ABC);
    let before = ids(&schema.fields, "", &[]);
    rename(field(&mut schema.fields, "c"), "FullName");
    let mut after = ids(&schema.fields, "", &[("FullName", "c")]);
    for id in &mut after {
        id.name = format!(" {} ", id.name);
    }
    let identity = SchemaIdentity { before, after };
    let saved = schema_yaml(Some(ABC), &schema, Some(&identity)).unwrap();
    assert_eq!(
        saved.text,
        ABC.replace(
            "  - keyword: c # c\n",
            "  - keyword: FullName # c\n    column: c\n"
        )
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
            array: false,
        });
    });
    assert_eq!(
        saved.text,
        "version: 1\ntable: t\nprimary_key: a\nfields:\n  - text: f0 # f\n    column: a\n    required: false\n    emit: false\n"
    );
}

// A `|+` scalar owns the blank lines after it: moved last, it must not take
// the blank line that followed its list as more content.

#[test]
fn an_item_ending_in_a_kept_block_scalar_moves_last_without_growing_it() {
    let base = "version: 1\ntable: t\nprimary_key: a\nfields:\n  - object: o\n    fields:\n      # about a\n      - text: a\n        column: a\n        required: false\n        default: |+\n          x\n\n      # about b\n      - text: b\n        column: a\n        required: true\n\n  - text: c\n    column: a\n    required: true\n";
    let saved = save_as(base, &[], |s| {
        let o = children(&mut s.fields, "o");
        let a = o.remove(0);
        o.push(a);
    });
    assert_eq!(
        saved.text,
        "version: 1\ntable: t\nprimary_key: a\nfields:\n  - object: o\n    fields:\n      # about b\n      - text: b\n        column: a\n        required: true\n      # about a\n      - text: a\n        column: a\n        required: false\n        default: |+\n          x\n\n  - text: c\n    column: a\n    required: true\n"
    );
}

// Sinks are keyed by name: a renamed one is renamed in place.

fn sink(name: &str) -> kernel::common::SinkName {
    kernel::common::SinkName::try_new(name).unwrap()
}

fn rename_sink(config: &mut ConfigToml, from: &str, to: &str) {
    let entry = config.sinks.remove(&sink(from)).unwrap();
    config.sinks.insert(sink(to), entry);
    for index in &mut config.index {
        if let Some(options) = index.sinks.remove(&sink(from)) {
            index.sinks.insert(sink(to), options);
        }
    }
}

/// The identity of a save that renames the sinks `renamed` lists (new, old)
/// and keeps every other `[[index]]` entry and sink where it is.
fn sink_identity(base: &str, edited: &ConfigToml, renamed: &[(&str, &str)]) -> ConfigIdentity {
    let was = ConfigToml::try_parse(base).unwrap();
    let names: Vec<String> = was.index.iter().map(|e| e.name.to_string()).collect();
    let sinks = edited
        .sinks
        .keys()
        .map(|name| {
            let name = name.to_string();
            let old = renamed
                .iter()
                .find(|(new, _)| *new == name)
                .map_or(name.clone(), |(_, old)| (*old).to_owned());
            let kept = was.sinks.contains_key(&sink(&old));
            (name, kept.then_some(old))
        })
        .collect();
    ConfigIdentity {
        after: (0..edited.index.len()).map(Some).collect(),
        before: names,
        sinks: Some(sinks),
    }
}

fn save_sinks(base: &str, renamed: &[(&str, &str)], edit: impl Fn(&mut ConfigToml)) -> Saved {
    let mut edited = ConfigToml::try_parse(base).unwrap();
    edit(&mut edited);
    let identity = sink_identity(base, &edited, renamed);
    save_config_with(base, Some(identity), edit)
}

#[test]
fn a_renamed_sink_keeps_its_place_keys_and_comments() {
    let saved = save_sinks(CONFIG, &[("main", "primary")], |c| {
        rename_sink(c, "primary", "main");
    });
    assert_eq!(
        saved.text,
        CONFIG.replace("[sinks.primary]", "[sinks.main]")
    );
    assert_eq!(saved.dropped, 0);
    let base = CONFIG.replace("[sinks.audit]", "# the audit trail\n[sinks.audit]");
    let saved = save_sinks(&base, &[("trail", "audit")], |c| {
        rename_sink(c, "audit", "trail");
    });
    assert_eq!(saved.text, base.replace("[sinks.audit]", "[sinks.trail]"));
}

#[test]
fn a_sink_identity_in_another_spelling_still_renames_in_place() {
    let identity = |sinks: &[(&str, Option<&str>)]| ConfigIdentity {
        before: ConfigToml::try_parse(CONFIG)
            .unwrap()
            .index
            .iter()
            .map(|e| e.name.to_string())
            .collect(),
        after: (0..3).map(Some).collect(),
        sinks: Some(
            sinks
                .iter()
                .map(|(is, was)| ((*is).to_owned(), was.map(str::to_owned)))
                .collect(),
        ),
    };
    let saved = save_config_with(
        CONFIG,
        Some(identity(&[
            ("primaryC", Some("primary")),
            (" audit", Some("AUDIT")),
        ])),
        |c| rename_sink(c, "primary", "primaryc"),
    );
    assert_eq!(
        saved.text,
        CONFIG.replace("[sinks.primary]", "[sinks.primaryc]")
    );
    assert_eq!(saved.dropped, 0);

    let not_a_name = save_config_with(
        CONFIG,
        Some(identity(&[
            ("primary c", Some("primary")),
            ("audit", Some("audit")),
        ])),
        |c| rename_sink(c, "primary", "primaryc"),
    );
    let twice = save_config_with(
        CONFIG,
        Some(identity(&[
            ("primaryc", Some("primary")),
            ("primaryC", Some("audit")),
        ])),
        |c| rename_sink(c, "primary", "primaryc"),
    );
    let by_name = save_config_with(CONFIG, None, |c| rename_sink(c, "primary", "primaryc"));
    assert_eq!(not_a_name.text, by_name.text);
    assert_eq!(twice.text, by_name.text);
    assert_ne!(by_name.text, saved.text);
}

#[test]
fn a_renamed_sink_renames_its_per_index_tables_in_place() {
    let base = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# a\n[sinks.a]\ntype = \"stdout\"\n\n[sinks.b]\ntype = \"stdout\"\n\n[[index]]\nname = \"users\"\nschema = \"users.schema.yml\"\nenabled = true\n# users in a\n[index.sinks.a]\nshards = 2 # two\n\n[[index]]\nname = \"orders\"\nschema = \"orders.schema.yml\"\nenabled = true\nsinks.a.shards = 3 # three\nsinks.b = { shards = 4 }\n";
    let saved = save_sinks(base, &[("z", "a")], |c| rename_sink(c, "a", "z"));
    assert_eq!(saved.text, base.replace("sinks.a", "sinks.z"));
    let saved = save_sinks(base, &[("a", "b"), ("b", "a")], |c| {
        let a = c.sinks.remove(&sink("a")).unwrap();
        let b = c.sinks.remove(&sink("b")).unwrap();
        c.sinks.insert(sink("a"), b);
        c.sinks.insert(sink("b"), a);
        for index in &mut c.index {
            let a = index.sinks.remove(&sink("a"));
            let b = index.sinks.remove(&sink("b"));
            if let Some(b) = b {
                index.sinks.insert(sink("a"), b);
            }
            if let Some(a) = a {
                index.sinks.insert(sink("b"), a);
            }
        }
    });
    assert_eq!(
        saved.text,
        "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# a\n[sinks.b]\ntype = \"stdout\"\n\n[sinks.a]\ntype = \"stdout\"\n\n[[index]]\nname = \"users\"\nschema = \"users.schema.yml\"\nenabled = true\n# users in a\n[index.sinks.b]\nshards = 2 # two\n\n[[index]]\nname = \"orders\"\nschema = \"orders.schema.yml\"\nenabled = true\nsinks.b.shards = 3 # three\nsinks.a = { shards = 4 }\n"
    );
}

#[test]
fn a_sink_removed_where_another_takes_its_name_goes_with_its_comments() {
    let saved = save_sinks(CONFIG, &[("audit", "primary")], |c| {
        c.sinks.remove(&sink("audit"));
        rename_sink(c, "primary", "audit");
    });
    assert!(
        saved.text.contains("[sinks.audit]\ntype = \"opensearch\""),
        "{}",
        saved.text
    );
    assert!(
        saved.text.contains("# auto_subfields = true\n"),
        "{}",
        saved.text
    );
    assert!(!saved.text.contains("type = \"stdout\""), "{}", saved.text);
    assert!(!saved.text.contains("pretty"), "{}", saved.text);
    assert_eq!(saved.dropped, 0);
}

#[test]
fn without_identity_a_renamed_sinks_comments_are_reported() {
    let mut config = ConfigToml::try_parse(CONFIG).unwrap();
    rename_sink(&mut config, "primary", "main");
    let saved = config_toml(Some(CONFIG), &config, None).unwrap();
    assert!(!saved.text.contains("# number_of_shards"), "{}", saved.text);
    assert_eq!(saved.dropped, 6, "{}", saved.text);
    let mut config = ConfigToml::try_parse(CONFIG).unwrap();
    config.sinks.remove(&sink("audit"));
    let saved = config_toml(Some(CONFIG), &config, None).unwrap();
    assert_eq!(saved.dropped, 0, "nothing was gained: audit was removed");
}

// The end of the file.

#[test]
fn a_comment_ending_the_file_below_the_last_table_is_that_tables() {
    let base = format!("{CONFIG}# end of file comment\n");
    let all = |n: usize| Some((0..n).map(Some).collect::<Vec<_>>());
    let saved = save_config_as(&base, all(2), |c| {
        c.index.pop();
    });
    assert!(!saved.text.contains("end of file"), "{}", saved.text);
    let saved = save_config_as(&base, Some(vec![Some(2), Some(0), Some(1)]), |c| {
        let orders = c.index.pop().unwrap();
        c.index.insert(0, orders);
    });
    assert!(
        saved.text.contains("name = \"orders\"\nschema = \"orders.schema.yml\"\nenabled = true\n# end of file comment\n"),
        "{}",
        saved.text
    );
    assert!(
        saved
            .text
            .ends_with("name = \"products\"\nschema = \"products.schema.yml\"\nenabled = true\n"),
        "{}",
        saved.text
    );
    let base = format!("{CONFIG}\n# a blank line away: the file's\n");
    let saved = save_config_as(&base, all(2), |c| {
        c.index.pop();
    });
    assert!(
        saved
            .text
            .ends_with("enabled = true\n\n# a blank line away: the file's\n"),
        "{}",
        saved.text
    );
}

#[test]
fn a_comment_ending_a_file_with_no_final_newline_stays_its_tables() {
    let base = format!("{CONFIG}# end of file comment");
    let moved = save_config_as(&base, Some(vec![Some(2), Some(0), Some(1)]), |c| {
        let orders = c.index.pop().unwrap();
        c.index.insert(0, orders);
    });
    assert!(
        moved
            .text
            .contains("enabled = true\n# end of file comment\n\n"),
        "{}",
        moved.text
    );
    let removed = save_config_as(&moved.text, Some(vec![Some(0), Some(1)]), |c| {
        c.index.pop();
    });
    assert!(
        removed.text.contains("# end of file comment"),
        "{}",
        removed.text
    );
    assert_eq!(removed.dropped, 0);
    let unchanged = save_config_as(&base, Some(vec![Some(0), Some(1), Some(2)]), |c| {
        c.index[0].enabled = !c.index[0].enabled;
    });
    assert!(
        unchanged
            .text
            .ends_with("enabled = true\n# end of file comment"),
        "{}",
        unchanged.text
    );
}

// A malformed identity is no identity: it pairs by name.

#[test]
fn a_malformed_config_identity_pairs_by_name() {
    let edit = |c: &mut ConfigToml| {
        c.index.remove(1);
    };
    let plain = save_config_with(CONFIG, None, edit).text;
    let names: Vec<String> = ["users", "products", "orders"].map(String::from).into();
    for after in [vec![Some(0), Some(0)], vec![Some(0), Some(7)]] {
        let identity = ConfigIdentity {
            before: names.clone(),
            after,
            sinks: None,
        };
        assert_eq!(save_config_with(CONFIG, Some(identity), edit).text, plain);
    }
    let rename = |c: &mut ConfigToml| rename_sink(c, "primary", "main");
    let plain = save_config_with(CONFIG, None, rename);
    let malformed = [
        [("audit", Some("audit")), ("main", Some("audit"))],
        [("audit", Some("audit")), ("main", Some("nope"))],
        [("audit", Some("audit")), ("other", Some("primary"))],
    ];
    for map in malformed {
        let identity = ConfigIdentity {
            before: names.clone(),
            after: vec![Some(0), Some(1), Some(2)],
            sinks: Some(
                map.iter()
                    .map(|(k, v)| ((*k).to_owned(), v.map(str::to_owned)))
                    .collect(),
            ),
        };
        let saved = save_config_with(CONFIG, Some(identity), rename);
        assert_eq!(
            (saved.text, saved.dropped),
            (plain.text.clone(), plain.dropped)
        );
    }
}

#[test]
fn a_malformed_schema_identity_pairs_by_name() {
    let edit = |s: &mut IndexSchema| {
        remove(&mut s.fields, "b");
        rename(field(&mut s.fields, "c"), "cc");
    };
    let mut schema = parse(ABC);
    edit(&mut schema);
    let plain = schema_yaml(Some(ABC), &schema, None).unwrap();
    let blank = |ids: Vec<FieldId>| {
        ids.into_iter()
            .map(|id| FieldId {
                uid: String::new(),
                ..id
            })
            .collect()
    };
    let same = |ids: Vec<FieldId>| {
        ids.into_iter()
            .map(|id| FieldId {
                uid: "x".into(),
                ..id
            })
            .collect()
    };
    for spoil in [blank as fn(Vec<FieldId>) -> Vec<FieldId>, same] {
        let identity = SchemaIdentity {
            before: spoil(ids(&parse(ABC).fields, "", &[])),
            after: spoil(ids(&schema.fields, "", &[("cc", "c")])),
        };
        let saved = schema_yaml(Some(ABC), &schema, Some(&identity)).unwrap();
        assert_eq!(
            (saved.text, saved.dropped),
            (plain.text.clone(), plain.dropped)
        );
    }
}

#[test]
fn a_kept_block_scalar_list_item_moved_up_keeps_its_value() {
    let base = "version: 1\ntable: t\nprimary_key: a\nfields:\n\n# about a\n- geo: a\n  lat: a\n  lon: a\n  required: true\n\n# about b\n- text: b\n  column: a\n  required: true\n  options:\n    k:\n    - |+\n      x\n  \n";
    let saved = save_as(base, &[], |s| s.fields.swap(0, 1));
    assert_eq!(
        saved.text,
        "version: 1\ntable: t\nprimary_key: a\nfields:\n\n# about b\n- text: b\n  column: a\n  required: true\n  options:\n    k:\n    - |+\n      x\n  \n# about a\n- geo: a\n  lat: a\n  lon: a\n  required: true\n"
    );
}

#[test]
fn a_new_field_ending_in_a_kept_block_scalar_does_not_take_the_blank_line_after_it() {
    let base = ABC.replace("  # about b", "\n  # about b");
    let added = parse("version: 1\ntable: t\nprimary_key: a\nfields:\n  - constant: k\n    value:\n      'a b': \"\\n\"\n")
        .fields
        .remove(0);
    let saved = save_as(&base, &[], |s| s.fields.insert(1, added));
    assert!(saved.text.contains("# about b"), "{}", saved.text);
}

#[test]
fn an_item_moved_to_the_top_of_its_list_leaves_its_blank_line_behind() {
    let base = ABC.replace("  # about c", "\n  # about c");
    let saved = save_as(&base, &[], |s| {
        let c = s.fields.pop().unwrap();
        s.fields.insert(0, c);
    });
    assert!(
        saved.text.contains("fields:\n  # about c\n"),
        "{}",
        saved.text
    );
}

// Line endings (issue #273).

/// The lines of `text`, each with its own line ending.
fn raw_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Whether every line of `text` ends in CRLF (a last line with no ending
/// aside).
fn all_crlf(text: &str) -> bool {
    raw_lines(text)
        .iter()
        .all(|l| !l.ends_with('\n') || l.ends_with("\r\n"))
}

/// `text` with every line ending in CRLF.
fn to_crlf(text: &str) -> String {
    text.replace('\n', "\r\n")
}

/// The raw lines (endings included) only `a` has and only `b` has.
fn changed_raw(a: &str, b: &str) -> (Vec<String>, Vec<String>) {
    let mark = |t: &str| {
        raw_lines(t)
            .iter()
            .map(|l| l.replace('\r', "\u{240d}").replace('\n', "\u{2424}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let (removed, added) = changed(&mark(a), &mark(b));
    let unmark = |v: Vec<String>| {
        v.into_iter()
            .map(|l| l.replace('\u{240d}', "\r").replace('\u{2424}', "\n"))
            .collect()
    };
    (unmark(removed), unmark(added))
}

fn set_option(config: &mut ConfigToml, name: &str, key: &str, value: &str) {
    config
        .sinks
        .get_mut(&sink(name))
        .unwrap()
        .port
        .options
        .insert(key, value);
}

#[test]
fn a_crlf_config_changes_exactly_the_edited_line() {
    let base = to_crlf(CONFIG);
    let text = save_config(&base, |c| set_option(c, "primary", "url", "http://os:9200"));
    assert_eq!(
        changed_raw(&base, &text),
        (
            vec!["url = \"http://127.0.0.1:9200\"\r\n".into()],
            vec!["url = \"http://os:9200\"\r\n".into()]
        )
    );
    assert_eq!(raw_lines(&text).len(), raw_lines(&base).len());
}

#[test]
fn a_crlf_config_writes_every_line_it_adds_or_moves_in_crlf() {
    let base = to_crlf(&format!("{CONFIG}# end of file\n"));
    let check = |saved: &Saved| {
        assert!(all_crlf(&saved.text), "{:?}", saved.text);
        assert!(saved.text.contains("# end of file\r\n"), "{:?}", saved.text);
    };
    check(&save_sinks(&base, &[], |c| {
        set_option(c, "audit", "note", "two\nlines");
    }));
    check(&save_sinks(&base, &[], |c| {
        let mut entry = c.index[0].clone();
        entry.name = kernel::common::IndexName::try_new("extra").unwrap();
        c.index.push(entry);
        c.sinks
            .insert(sink("third"), config::toml::SinkEntry::new("stdout"));
        c.index[1]
            .sinks
            .insert(sink("third"), kernel::Options::empty());
    }));
    let saved = save_sinks(&base, &[("main", "primary")], |c| {
        rename_sink(c, "primary", "main")
    });
    check(&saved);
    assert!(
        saved.text.contains("\r\n[sinks.main]\r\n"),
        "{:?}",
        saved.text
    );
    let saved = save_config_as(&base, Some(vec![Some(2), Some(0), Some(1)]), |c| {
        let orders = c.index.pop().unwrap();
        c.index.insert(0, orders);
    });
    check(&saved);
    let saved = save_sinks(&base, &[], |c| {
        c.sinks.remove(&sink("primary"));
    });
    check(&saved);
    assert!(!saved.text.contains("number_of_shards"), "{:?}", saved.text);
}

#[test]
fn a_mixed_config_keeps_each_untouched_line_and_writes_new_ones_in_the_dominant_ending() {
    let base = to_crlf(CONFIG)
        .replace("tls_verify = false\r\n", "tls_verify = false\n")
        .replace(
            "# Three indexes built from the same store, each a different root table.\r\n",
            "# Three indexes built from the same store, each a different root table.\n",
        );
    let text = save_config(&base, |c| {
        let mut entry = c.index[0].clone();
        entry.name = kernel::common::IndexName::try_new("extra").unwrap();
        c.index.push(entry);
    });
    let (removed, added) = changed_raw(&base, &text);
    assert!(removed.is_empty(), "{removed:?}");
    assert!(added.iter().all(|l| l.ends_with("\r\n")), "{added:?}");
    assert!(
        added.contains(&"name = \"extra\"\r\n".to_owned()),
        "{added:?}"
    );
}

#[test]
fn a_multi_line_string_keeps_its_own_line_breaks() {
    let base = "[source]\r\ntype = \"postgres\"\r\nconnection_url = \"postgres://x\"\r\n\r\n[sinks.audit]\r\ntype = \"stdout\"\r\nnote = \"\"\"\nkept\nas is\"\"\"\r\nwin = \"\"\"\r\nkept\r\nhere\"\"\"\r\n";
    let text = save_config(base, |c| set_option(c, "audit", "pretty", "yes"));
    assert_eq!(text, format!("{base}pretty = \"yes\"\r\n"));
}

#[test]
fn an_lf_config_holding_the_stand_in_character_is_patched_in_place() {
    let base = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# \u{e00d} kept\n[sinks.audit]\ntype = \"stdout\"\nnote = \"a\u{e00d}b\"\n";
    let text = save_config(base, |c| set_option(c, "audit", "pretty", "yes"));
    assert_eq!(text, format!("{base}pretty = \"yes\"\n"));
}

#[test]
fn a_crlf_config_without_a_final_line_ending_keeps_it_that_way() {
    let base = to_crlf(CONFIG);
    let base = base.strip_suffix("\r\n").unwrap();
    let text = save_config(base, |c| c.index[0].enabled = false);
    assert!(all_crlf(&text), "{text:?}");
    let text = save_config(&format!("{base}\r\n# end"), |c| {
        c.index.pop();
    });
    assert!(all_crlf(&text), "{text:?}");
}

#[test]
fn a_crlf_config_written_in_the_designers_form_keeps_crlf() {
    let base = "x = 1\r\n";
    let config = ConfigToml::try_parse(&to_crlf(CONFIG)).unwrap();
    let saved = config_toml(Some(base), &config, None).unwrap();
    assert_eq!(saved.notice, Some(Notice::LayoutReset));
    assert_eq!(
        saved.text,
        to_crlf(&codegen::config_to_toml(&config).unwrap())
    );
}

#[test]
fn a_crlf_schema_keeps_crlf_through_an_added_deleted_or_moved_field() {
    let base = to_crlf(ORDERS);
    let edits: [&dyn Fn(&mut IndexSchema); 3] = [
        &|s| {
            let mut extra = s.fields[1].clone();
            rename(&mut extra, "otherId");
            s.fields.insert(2, extra);
        },
        &|s| s.fields.retain(|f| f.field.as_ref() != "timeline"),
        &|s| s.fields.swap(0, 3),
    ];
    for edit in edits {
        let text = save(&base, edit);
        assert!(all_crlf(&text), "{text:?}");
        let (removed, added) = changed_raw(&base, &text);
        assert!(removed.iter().chain(&added).all(|l| l.ends_with("\r\n")));
    }
}

#[test]
fn a_crlf_block_scalar_keeps_crlf() {
    let base = "version: 1\r\ntable: t\r\nprimary_key: a\r\nfields:\r\n  - text: b\r\n    column: a\r\n    required: true\r\n    options:\r\n      k:\r\n      - |\r\n        x\r\n        y\r\n  - integer: c # c\r\n    required: true\r\n";
    let text = save(base, |s| {
        rename(&mut s.fields[1], "d");
        s.fields.swap(0, 1);
    });
    assert!(all_crlf(&text), "{text:?}");
    assert!(
        text.contains("      - |\r\n        x\r\n        y\r\n"),
        "{text:?}"
    );
}

#[test]
fn a_crlf_schema_written_in_the_designers_form_keeps_crlf() {
    let base = "{version: 1, table: t, primary_key: id, fields: [{integer: id, required: true}]} # flow\r\n";
    let mut schema = parse(base);
    rename(&mut schema.fields[0], "key");
    let saved = schema_yaml(Some(base), &schema, None).unwrap();
    assert_eq!(saved.notice, Some(Notice::LayoutReset));
    assert_eq!(
        saved.text,
        to_crlf(&codegen::schema_to_yaml(&schema).unwrap())
    );
}

// Names the loader normalizes (issue #273).

const SPELLED: &str = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n\n# the primary\n[sinks.Primary]\ntype = \"stdout\"\npretty = true\n# below the primary\n\n[sinks.b]\ntype = \"stdout\"\n\n[[index]]\nname = \" Users \"\nschema = \"users.schema.yml\"\nenabled = true\n# users in Primary\n[index.sinks.PRIMARY]\nshards = 2\n\n[[index]]\nname = \"Orders\"\nschema = \"orders.schema.yml\"\nenabled = true\nsinks.Primary.shards = 3 # three\n\n[[index]]\nname = \"items\"\nschema = \"items.schema.yml\"\nenabled = true\nsinks = { Primary = { shards = 4 } }\n";

fn set_shards(config: &mut ConfigToml, index: usize, shards: i64) {
    config.index[index]
        .sinks
        .get_mut(&sink("primary"))
        .unwrap()
        .insert("shards", shards);
}

#[test]
fn a_sink_spelled_in_another_case_is_edited_where_it_stands() {
    let text = save_config(SPELLED, |c| {
        c.sinks
            .get_mut(&sink("primary"))
            .unwrap()
            .port
            .options
            .insert("pretty", false);
        set_shards(c, 0, 5);
        set_shards(c, 1, 6);
        set_shards(c, 2, 7);
    });
    assert_eq!(
        text,
        SPELLED
            .replace("pretty = true", "pretty = false")
            .replace("shards = 2", "shards = 5")
            .replace("shards = 3", "shards = 6")
            .replace("shards = 4", "shards = 7")
    );
}

#[test]
fn a_sink_spelled_in_another_case_is_renamed_where_it_stands() {
    let saved = save_sinks(SPELLED, &[("main", "primary")], |c| {
        rename_sink(c, "primary", "main")
    });
    assert_eq!(
        saved.text,
        SPELLED
            .replace("[sinks.Primary]", "[sinks.main]")
            .replace("[index.sinks.PRIMARY]", "[index.sinks.main]")
            .replace("sinks.Primary.shards", "sinks.main.shards")
            .replace("{ Primary =", "{ main =")
    );
    assert_eq!(saved.dropped, 0);
}

#[test]
fn a_sink_spelled_in_another_case_is_removed_with_its_comments() {
    let saved = save_sinks(SPELLED, &[], |c| {
        c.sinks.remove(&sink("primary"));
        for index in &mut c.index {
            index.sinks.remove(&sink("primary"));
        }
    });
    let text = &saved.text;
    assert!(!text.to_lowercase().contains("primary"), "{text}");
    assert!(text.contains("\n[sinks.b]\n"), "{text}");
    assert_eq!(saved.dropped, 0);
}

#[test]
fn an_index_named_in_another_spelling_is_patched_in_place() {
    let text = save_config(SPELLED, |c| c.index[1].enabled = false);
    assert_eq!(
        changed(SPELLED, &text),
        (
            vec!["enabled = true".into()],
            vec!["enabled = false".into()]
        )
    );
    let text = save_config(SPELLED, |c| {
        c.index.remove(0);
    });
    assert!(!text.contains("Users"), "{text}");
    assert!(text.contains("name = \"Orders\""), "{text}");
    let text = save_config_as(SPELLED, Some(vec![Some(1), Some(0), Some(2)]), |c| {
        c.index.swap(0, 1)
    })
    .text;
    assert!(
        text.find("\"Orders\"").unwrap() < text.find("\" Users \"").unwrap(),
        "{text}"
    );
    assert!(text.contains("name = \" Users \"\nschema"), "{text}");
}

#[test]
fn a_sink_spelled_twice_is_written_in_the_designers_form() {
    for base in [
        SPELLED.replace("[sinks.b]", "[sinks.primary]"),
        SPELLED.replace(
            "sinks.Primary.shards = 3",
            "sinks.Primary.shards = 3\nsinks.primary.replicas = 1",
        ),
    ] {
        // The loader takes the later table and drops the earlier one.
        let mut config = ConfigToml::try_parse(&base).unwrap();
        config.index[2].enabled = false;
        let saved = config_toml(Some(&base), &config, None).unwrap();
        assert_eq!(saved.notice, Some(Notice::LayoutReset), "{}", saved.text);
    }
}

// Layout a save keeps (issue #279).

const BARE: &str = "[source]\ntype = \"postgres\"\nconnection_url = \"postgres://x\"\n";

#[test]
fn a_byte_order_mark_stays() {
    let edit = |c: &mut ConfigToml| c.index[0].enabled = false;
    let text = save_config(&format!("\u{feff}{CONFIG}"), edit);
    assert_eq!(text, format!("\u{feff}{}", save_config(CONFIG, edit)));
    let edit = |s: &mut IndexSchema| rename(&mut s.fields[0], "key");
    let text = save(&format!("\u{feff}{ORDERS}"), edit);
    assert_eq!(text, format!("\u{feff}{}", save(ORDERS, edit)));

    let bare = format!("\u{feff}{BARE}");
    assert_eq!(save_config(&bare, |_| {}), bare);

    let base = "\u{feff}x = 1\r\n";
    let config = ConfigToml::try_parse(CONFIG).unwrap();
    let saved = config_toml(Some(base), &config, None).unwrap();
    assert_eq!(saved.notice, Some(Notice::LayoutReset));
    assert_eq!(
        saved.text,
        format!(
            "\u{feff}{}",
            to_crlf(&codegen::config_to_toml(&config).unwrap())
        )
    );
}

#[test]
fn editing_a_top_level_dotted_key_keeps_the_blank_line_below_it() {
    let base =
        format!("# head\n\nsinks.primary.type = \"stdout\"\nsinks.primary.pretty = true\n\n{BARE}");
    let text = save_config(&base, |c| set_option(c, "primary", "pretty", "no"));
    assert_eq!(text, base.replace("pretty = true", "pretty = \"no\""));
    let base = format!("sinks.primary.type = \"stdout\"\n\n{BARE}");
    let text = save_config(&base, |c| set_option(c, "primary", "pretty", "no"));
    assert_eq!(
        text,
        base.replace(
            "\"stdout\"\n",
            "\"stdout\"\nsinks.primary.pretty = \"no\"\n"
        )
    );
}

#[test]
fn a_key_added_to_an_inline_table_follows_its_spacing() {
    let base = format!(
        "{BARE}\n[sinks]\na = {{ type = \"stdout\", note = \"q\" }}\nb = {{type=\"stdout\"}}\nc = {{ type = \"stdout\", note = \"q\" }} # c\n"
    );
    let text = save_config(&base, |c| {
        set_option(c, "a", "pretty", "yes");
        set_option(c, "b", "pretty", "yes");
        c.sinks
            .get_mut(&sink("c"))
            .unwrap()
            .port
            .options
            .0
            .remove("note");
    });
    assert_eq!(
        text,
        base.replace("\"q\" }\nb", "\"q\", pretty = \"yes\" }\nb")
            .replace("{type=\"stdout\"}", "{type=\"stdout\",pretty=\"yes\"}")
            .replace(
                "{ type = \"stdout\", note = \"q\" } # c",
                "{ type = \"stdout\" } # c"
            )
    );
}

#[test]
fn an_untouched_dotted_key_keeps_its_spacing() {
    let base = format!(
        "{BARE}\n[sinks.p]\ntype = \"stdout\"\nx.y = 1\n  x . z = \"w\" # z\nx .\tv = 2\n\n[sinks.q]\ntype = \"stdout\"\n"
    );
    let text = save_config(&base, |c| set_option(c, "q", "pretty", "yes"));
    assert_eq!(text, format!("{base}pretty = \"yes\"\n"));
    let text = save_config(&base, |c| set_option(c, "p", "pretty", "yes"));
    assert_eq!(text, base.replace("2\n\n", "2\npretty = \"yes\"\n\n"));
    let text = save_config(&base, |c| {
        let x = [
            ("y".to_owned(), 1i64.into()),
            ("z".to_owned(), "v".into()),
            ("v".to_owned(), 2i64.into()),
        ];
        let x = kernel::OptionValue::Map(x.into_iter().collect());
        c.sinks
            .get_mut(&sink("p"))
            .unwrap()
            .port
            .options
            .insert("x", x);
    });
    assert_eq!(text, base.replace("z = \"w\"", "z = \"v\""));
}

#[test]
fn a_crlf_string_with_a_line_ending_backslash_is_patched_in_place() {
    let base = "[source]\r\ntype = \"postgres\"\r\nconnection_url = \"postgres://x\"\r\n\r\n# p\r\n[sinks.p]\r\ntype = \"stdout\"\r\nnote = \"\"\"a \\  \t\r\n\r\n  b \\\r\n c\\\\\r\nd\"\"\"\r\nmixed = \"\"\"x\\\n\r\n \r\n y\\\r\n\n\t\"\"\"\r\nsame = [\"\"\"a \\\r\n  b\"\"\", \"\"\"a \\\r\n  b\"\"\"] # s\r\n";
    let text = save_config(base, |c| set_option(c, "p", "pretty", "yes"));
    assert_eq!(text, format!("{base}pretty = \"yes\"\r\n"));
    let text = save_config(base, |c| set_option(c, "p", "mixed", "z"));
    assert_eq!(
        text,
        base.replace(
            "mixed = \"\"\"x\\\n\r\n \r\n y\\\r\n\n\t\"\"\"\r\n",
            "mixed = \"z\"\r\n"
        )
    );
}

#[test]
fn a_value_with_characters_toml_forbids_raw_is_escaped() {
    let forbidden: String = (0u8..=0x1f)
        .chain([0x7f])
        .filter(|b| *b != b'\t')
        .map(char::from)
        .collect();
    for base in [
        format!("{BARE}\n[sinks.p]\ntype = \"stdout\"\n"),
        to_crlf(&format!("{BARE}\n[sinks.p]\ntype = \"stdout\"\n")),
    ] {
        save_config(&base, |c| set_option(c, "p", "note", forbidden.as_str()));
        save_config(&base, |c| set_option(c, "p", "note", "a\nb\u{7f}"));
        save_config(&base, |c| set_option(c, "p", "note", "a\u{7f}\t"));
    }
    let base = to_crlf(&format!("{BARE}\n[sinks.p]\ntype = \"stdout\"\n"));
    let text = save_config(&base, |c| set_option(c, "p", "note", "a\nb\u{7f}"));
    assert_eq!(text, format!("{base}note = \"a\\nb\\u007F\"\r\n"));
}

#[test]
fn a_mixed_schema_keeps_each_untouched_line_and_writes_new_ones_in_the_dominant_ending() {
    let base = to_crlf(ORDERS).replacen("\r\n", "\n", 3);
    let text = save(&base, |s| rename(&mut s.fields[0], "key"));
    assert_eq!(
        changed_raw(&base, &text),
        (
            vec!["  - integer: id\r\n".into()],
            vec!["  - integer: key\r\n".into(), "    column: id\r\n".into()]
        )
    );
    let base = ORDERS.replacen('\n', "\r\n", 3);
    let text = save(&base, |s| {
        let mut extra = s.fields[1].clone();
        rename(&mut extra, "otherId");
        s.fields.insert(2, extra);
    });
    let (removed, added) = changed_raw(&base, &text);
    assert!(removed.is_empty(), "{removed:?}");
    assert!(added.iter().all(|l| !l.contains('\r')), "{added:?}");
    assert!(text.starts_with(&base[..base.match_indices('\n').nth(3).unwrap().0]));
}

#[test]
fn a_moved_field_keeps_the_endings_of_its_own_lines() {
    let base = "version: 1\r\ntable: t\r\nprimary_key: a\r\nfields:\r\n  - keyword: a\r\n    required: true\r\n  - keyword: b\n    required: true\n  - keyword: c\r\n    required: true\r\n";
    let text = save(base, |s| s.fields.swap(0, 1));
    assert_eq!(
        text,
        "version: 1\r\ntable: t\r\nprimary_key: a\r\nfields:\r\n  - keyword: b\n    required: true\n  - keyword: a\r\n    required: true\r\n  - keyword: c\r\n    required: true\r\n"
    );
}
