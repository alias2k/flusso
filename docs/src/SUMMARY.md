# Summary

[Introduction](introduction.md)

# Start here

- [How flusso works](start/how-it-works.md)
- [Quickstart](start/quickstart.md)

# Author

- [Your first schema](author/first-schema.md)
- [Fold in related tables](author/related-tables.md)
- [Filter rows and soft-delete](author/filters-and-soft-delete.md)
- [Geo, maps, enums and transforms](author/geo-maps-enums-transforms.md)
- [Design a schema visually](author/design-visually.md)
- [Designer reference](author/designer-reference.md)

# Deploy

- [Write flusso.toml](deploy/flusso-toml.md)
- [Your own Postgres and OpenSearch](deploy/own-postgres-opensearch.md)
- [Compile and run](deploy/compile-and-run.md)
- [Ship with Docker](deploy/docker.md)
- [Deploy with Helm](deploy/helm.md)

# Operate

- [Watch it run](operate/watch-it-run.md)
- [Ship traces over OTLP](operate/traces-otlp.md)
- [Reindex without downtime](operate/reindex.md)
- [Handle rejected documents](operate/rejected-documents.md)
- [Recover from a dropped slot](operate/dropped-slot.md)
- [Secure the private surface](operate/private-surface.md)

# Query

- [Overview](query/overview.md)
- [Document structs](query/document-structs.md)
- [Field handles](query/field-handles.md)
- [Composing queries and options](query/composing.md)
- [Nested collections](query/nested.md)
- [Sorting](query/sorting.md)
- [Maps](query/maps.md)
- [Enums and custom values](query/enums-and-values.md)
- [Several indexes](query/several-indexes.md)
- [Results and the escape hatch](query/results-and-escape-hatch.md)
- [Binding to the schema](query/binding.md)
- [Migrating from path](query/migrating-from-path.md)

# Reference

- [flusso.toml top level](reference/config-toml.md)
- [Source: Postgres](reference/source-postgres.md)
- [Stream: channel](reference/stream-channel.md)
- [Sink: OpenSearch](reference/sink-opensearch.md)
- [Sink: stdout](reference/sink-stdout.md)
- [Index entries and on_error](reference/index-and-on-error.md)
- [Environment variables](reference/environment.md)
- [CLI](reference/cli.md)
- [flusso.lock](reference/lock.md)
- [Helm chart values](reference/helm-values.md)
- [Schema top-level keys](reference/schema-top-level.md)
- [Field types](reference/field-types.md)
- [Objects and maps](reference/objects-and-maps.md)
- [Joins](reference/joins.md)
- [Aggregates](reference/aggregates.md)
- [Filters and soft_delete](reference/filters-and-soft-delete.md)
- [Identifiers and validation](reference/identifiers.md)
- [Metrics](reference/metrics.md)
- [HTTP endpoints](reference/http.md)
- [Glossary](reference/glossary.md)

# Contribute

- [Architecture](contribute/architecture.md)
- [The pipeline](contribute/pipeline.md)
- [The config layer](contribute/config-layer.md)
- [Testing](contribute/testing.md)
- [Releasing](contribute/releasing.md)
- [Writing docs](contribute/writing-docs.md)
