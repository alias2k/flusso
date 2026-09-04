# flusso

flusso keeps a search destination in sync with a relational source from declarative config: it derives the shape of each search document, seeds it, then follows the source's change feed so the destination stays current. This glossary fixes the words the codebase, the manual, and the issues use for that. Decisions live in `docs/adr/`.

## Language

### Shape of the system

**Kernel**:
The vocabulary every other part trades in: values, validated names, index schemas and mappings, the envelope, the position. It names no adapter and no file format.
_Avoid_: core, schema-core, common

**Port**:
A contract the engine drives without knowing who implements it. There are exactly three: source, stream, sink.
_Avoid_: trait crate, abstraction layer, interface crate

**Adapter**:
A concrete implementation of one port for one technology (Postgres, an in-process channel, OpenSearch, stdout, NATS). An adapter owns its own configuration.
_Avoid_: backend, driver, plugin, connector

**Engine**:
A generic loop that drives ports. There are two: the ingest engine and the sink engine.
_Avoid_: pipeline (as a component name), worker, runner

**Daemon**:
The supervisor that assembles one deployment from a config: one ingest engine, one sink engine per sink, the lanes between them. It restarts a failed engine and exposes operations.
_Avoid_: runtime, service, orchestrator

**Deployment**:
One running flusso: one source, one stream, its sinks, and the indexes it maintains. A second source is a second deployment.
_Avoid_: instance, cluster, installation

### Ports

**Source**:
The port that captures changes as they happen, snapshots current rows on request, and builds documents. Backed by one replication slot.
_Avoid_: database, upstream, origin, producer

**Stream**:
The port in the middle: it carries envelopes from the ingest engine down to each sink and requests from sink engines back up. flusso's "stream" is this port; when NATS's own stream must be distinguished, call that a "JetStream stream".
_Avoid_: queue, bus, broker, channel (as the port name)

**Lane**:
One sink's ordered feed inside the stream. A stacked sink's lane is fed by the sink engine ahead of it instead of by the ingest engine.
_Avoid_: channel, topic, subject, partition

**Request lane**:
The stream's upward feed. It carries backfill and reindex requests from sink engines to the ingest engine, which coalesces requests for the same index.
_Avoid_: control channel, command bus, callback

**Sink**:
The port that applies documents to a destination and reports which it accepted, rejected, or already holds. Each sink has its own engine, its own seeding, its own reindex.
_Avoid_: destination, target, output, writer

### What flows

**Change**:
A source event naming a row that was inserted, updated, or deleted, identified by table and key only. Never carries row contents.
_Avoid_: event, mutation, delta, CDC record

**Position**:
An opaque, serializable offset in the source's change feed. The source confirms a position once every lane has acknowledged it.
_Avoid_: LSN, offset, cursor, ack, checkpoint

**Watermark**:
The lowest position every lane has acknowledged. The stream owns it; the ingest engine forwards it to the source as confirmation.
_Avoid_: low-water mark, min ack, commit point

**Document**:
One denormalized, typed search record built from a root row and its joins and aggregates, keyed by a deterministic id. Built once, on the ingest side.
_Avoid_: record, row, entity, payload

**Envelope**:
The message a lane carries: the index, the operation, the document id, the document, and the position. Emitting sinks forward it as-is.
_Avoid_: message, event, wrapper, frame

**Rejected document**:
A document the destination refused individually inside an otherwise applied batch. A stacked sink withholds it downstream.
_Avoid_: failed doc, error item, dead letter

**Quarantine**:
What happens to a rejected document under a skip policy: it is reported and left out so the batch can be confirmed.
_Avoid_: dead-letter, dropped, skipped (as a noun)

### Index lifecycle

**Index**:
One search document type defined by a schema, maintained in every sink of the deployment.
_Avoid_: collection, table, type

**Schema**:
The declarative description of an index: its root table, fields, joins, aggregates, and filters.
_Avoid_: definition, spec, config (for the index file)

**Mapping**:
The typed destination layout derived from a schema.
_Avoid_: index template, layout, projection

**Generation**:
One physical copy of an index in a destination. A reindex builds a fresh generation and swaps it in.
_Avoid_: version, revision, copy

**Seeded**:
A sink's own record that it holds a complete snapshot of an index for the current schema. Decided per sink, never for the deployment.
_Avoid_: initialized, bootstrapped, warm

**Backfill**:
A sink engine's request for a full snapshot of an index into its own lane, made when its sink is not seeded.
_Avoid_: initial load, seed (as a verb for the whole process), bootstrap

**Reindex**:
A forced backfill into a fresh generation of one index on one sink or on all of them. Per sink; it never restarts a sibling.
_Avoid_: rebuild, resync, full refresh

**Stack**:
An opt-in chain between sinks: the downstream sink's lane is fed by the upstream's engine with the documents the upstream accepted. Declaration order otherwise means nothing.
_Avoid_: pipeline, cascade, ordering, dependency

### Roles

**Operation**:
Something done to a running deployment, owned by the daemon: reindex, later pause or resume.
_Avoid_: command, action, admin call

**Primitive**:
Something one engine can do to its own ports: stage a generation, request a snapshot, drain.
_Avoid_: operation, step, hook

**Transport**:
How the outside world reaches the daemon: HTTP surfaces, process signals, telemetry export. Owned by the binary, never by the daemon.
_Avoid_: API layer, server, interface
