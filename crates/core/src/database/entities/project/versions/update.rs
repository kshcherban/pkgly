use sqlx::types::Json;
use uuid::Uuid;

use super::{DBProjectVersion, DBProjectVersionColumn};
use crate::{
    database::entities::package_file::DBPackageFile,
    database::prelude::*,
    repository::project::{ReleaseType, VersionData},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVersion {
    pub project_id: Uuid,
    pub repository_id: Uuid,
    /// The version of the project
    pub version: String,
    /// Release type
    pub release_type: ReleaseType,
    /// The path to the release
    pub version_path: String,
    /// The publisher of the version
    pub publisher: Option<i32>,
    /// The version page. Such as a README
    pub version_page: Option<String>,
    /// The version data. More data can be added in the future and the data can be repository dependent
    pub extra: VersionData,
}
impl NewVersion {
    pub async fn insert(self, db: &PgPool) -> Result<DBProjectVersion, sqlx::Error> {
        let Self {
            project_id,
            repository_id,
            version,
            release_type,
            version_path,
            publisher,
            version_page,
            extra,
        } = self;
        let db_version = InsertQueryBuilder::new(DBProjectVersion::table_name())
            .insert(DBProjectVersionColumn::ProjectId, project_id.value())
            .insert(DBProjectVersionColumn::RepositoryId, repository_id.value())
            .insert(DBProjectVersionColumn::Version, version.value())
            .insert(DBProjectVersionColumn::ReleaseType, release_type.value())
            .insert(DBProjectVersionColumn::Path, version_path.value())
            .insert(DBProjectVersionColumn::Publisher, publisher.value())
            .insert(DBProjectVersionColumn::VersionPage, version_page.value())
            .insert(DBProjectVersionColumn::Extra, Json(extra).value())
            .return_all()
            .query_as()
            .fetch_one(db)
            .await?;

        let _ = DBPackageFile::upsert_from_project_version(db, &db_version).await?;

        Ok(db_version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateProjectVersion {
    pub release_type: Option<ReleaseType>,
    pub publisher: Option<Option<i32>>,
    pub version_page: Option<Option<String>>,
    pub extra: Option<VersionData>,
}
impl UpdateProjectVersion {
    pub async fn update(&self, version_id: Uuid, database: &PgPool) -> DBResult<()> {
        let mut update = UpdateQueryBuilder::new(DBProjectVersion::table_name());
        self.apply_update_fields(version_id, &mut update);
        update.query().execute(database).await?;

        let version = sqlx::query_as::<_, DBProjectVersion>(
            r#"SELECT * FROM project_versions WHERE id = $1"#,
        )
        .bind(version_id)
        .fetch_one(database)
        .await?;
        let _ = DBPackageFile::upsert_from_project_version(database, &version).await?;

        Ok(())
    }

    fn apply_update_fields<'args>(&self, version_id: Uuid, update: &mut UpdateQueryBuilder<'args>) {
        let release_type = self.release_type.clone();
        let extra = self.extra.clone();
        let version_page = self.version_page.clone();
        let publisher = self.publisher;

        update
            .filter(DBProjectVersionColumn::Id.equals(version_id.value()))
            .set(DBProjectVersionColumn::UpdatedAt, SqlFunctionBuilder::now());

        if let Some(release_type) = release_type {
            update.set(DBProjectVersionColumn::ReleaseType, release_type);
        }
        if let Some(extra) = extra {
            update.set(DBProjectVersionColumn::Extra, Json(extra));
        }
        if let Some(version_page) = version_page {
            update.set(DBProjectVersionColumn::VersionPage, version_page.value());
        }
        if let Some(publisher) = publisher {
            update.set(DBProjectVersionColumn::Publisher, publisher);
        }
    }
}

#[cfg(test)]
mod tests;
