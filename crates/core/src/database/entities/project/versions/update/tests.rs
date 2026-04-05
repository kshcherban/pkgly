#![allow(
    clippy::expect_used,
    clippy::field_reassign_with_default,
    clippy::panic,
    clippy::todo,
    clippy::unwrap_used
)]
use super::*;
use uuid::Uuid;

#[test]
fn update_query_should_filter_by_version_id() {
    let mut builder = UpdateQueryBuilder::new(DBProjectVersion::table_name());
    let mut update = UpdateProjectVersion::default();
    update.extra = Some(VersionData::default());
    let version_id = Uuid::new_v4();
    update.apply_update_fields(version_id, &mut builder);

    let sql = builder.format_sql_query().to_string();
    assert!(
        sql.contains("WHERE project_versions.id = $1"),
        "expected filter on version id, got {sql}"
    );
}
