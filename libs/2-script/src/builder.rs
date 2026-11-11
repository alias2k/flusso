//! [`ScriptedBuilder`]: any [`DocumentBuilder`] with the deployment's scripts run over what it
//! builds.
//!
//! It sits between the source's builder and the engine, so every document is finished once,
//! before it is published to any sink. Resolution and everything else pass straight through: a
//! script reads only the built document, so it adds no dependency on any table.
//!
//! A script that fails turns the upsert into [`Document::Failed`]; the engine's failure policy
//! for the index decides what happens next. A tombstone is passed through untouched.
//!
//! ```
//! use std::sync::Arc;
//! use script::{ScriptedBuilder, Scripts};
//! use source::document::DocumentBuilder;
//!
//! fn wrap(inner: Arc<dyn DocumentBuilder>, scripts: Scripts) -> Arc<dyn DocumentBuilder> {
//!     ScriptedBuilder::wrap(inner, scripts)
//! }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use kernel::{IndexMapping, TableName};
use source::document::{Document, DocumentBuilder, DocumentId, IndexScope};
use source::{Result, RowImage, RowKey};

use crate::Scripts;

/// A [`DocumentBuilder`] that runs scripts over each document `inner` builds.
#[derive(Debug)]
pub struct ScriptedBuilder {
    inner: Arc<dyn DocumentBuilder>,
    scripts: Scripts,
}

impl ScriptedBuilder {
    /// Wrap `inner`, or hand it back unchanged when no index has a script or an
    /// input-only field.
    pub fn wrap(inner: Arc<dyn DocumentBuilder>, scripts: Scripts) -> Arc<dyn DocumentBuilder> {
        if scripts.is_empty() {
            inner
        } else {
            Arc::new(Self { inner, scripts })
        }
    }

    fn finish(&self, document: Document) -> Document {
        match document {
            Document::Upsert { id, mut body } => match self.scripts.apply(&id.index, &mut body) {
                Ok(()) => Document::Upsert { id, body },
                Err(failure) => Document::Failed { id, failure },
            },
            other => other,
        }
    }
}

#[async_trait]
impl DocumentBuilder for ScriptedBuilder {
    async fn resolve(
        &self,
        table: &TableName,
        key: &RowKey,
        before: Option<&RowImage>,
        after: Option<&RowImage>,
    ) -> Result<Vec<DocumentId>> {
        self.inner.resolve(table, key, before, after).await
    }

    async fn build(&self, id: &DocumentId) -> Result<Document> {
        Ok(self.finish(self.inner.build(id).await?))
    }

    async fn build_many(&self, ids: &[DocumentId]) -> Result<Vec<Document>> {
        let documents = self.inner.build_many(ids).await?;
        Ok(documents.into_iter().map(|d| self.finish(d)).collect())
    }

    fn backfill_scopes(&self) -> Vec<IndexScope> {
        self.inner.backfill_scopes()
    }

    async fn index_mappings(&self) -> Result<Vec<IndexMapping>> {
        self.inner.index_mappings().await
    }
}

#[cfg(test)]
mod tests;
