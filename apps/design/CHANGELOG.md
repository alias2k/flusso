# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.18.0](https://github.com/alias2k/flusso/compare/flusso-design-v0.17.0...flusso-design-v0.18.0) - 2026-11-14

### Added

- *(design)* author array columns ([#276](https://github.com/alias2k/flusso/pull/276))
- *(kernel)* a column may hold an array of its type ([#276](https://github.com/alias2k/flusso/pull/276))
- *(design)* a save keeps the file's comments and untouched layout

### Fixed

- *(cli)* a refusal stops the start only when every sink refuses ([#316](https://github.com/alias2k/flusso/pull/316))
- *(source)* [**breaking**] a schema that can never index a document stops the start (#316, #309)
- *(design)* a step whose file is at its destination ran, whatever is at its source
- *(design)* flush every rename before the record marks it
- *(design)* a rollback records each step's completion, never infers it
- *(design)* a rollback settles each file once, even across kills
- *(design)* Validate, a row's remove and the palette's New index apply a pending name first
- *(design,source)* a name applies when the press that left its box ends; key lists by identity
- *(source,design)* duplicate-name outcomes for constants and binary; one-line node header; palette returns focus
- *(design)* refuse a field name its level already has; warn about duplicates; read typed table and column names as flusso does
- *(design)* the Italian unsaved-notice line says "once the designer stops"
- *(design)* never keep a settled save's journal; a broken config names nothing outside
- *(design)* name files outside the folder, tell before the journal only when stored
- *(design)* tell a recovery before its journal goes; check notices on each load
- *(design)* a dismissed rollback's backup line holds for a deleted file too
- *(design)* keep the recovery notice until it is dismissed, and say what happened to each file
- *(cli)* script run reports a document that fails to build as the failure
- *(query-derive)* two refusals suggest a fix that works
- *(design)* name why leaving Code mode is held; refuse an op's unknown fields
- *(design)* drop the raw save op, say why a Code-mode save is refused
- *(design)* a save's response adds to a recovery notice on screen; the manual states how a committed save finishes
- *(design)* keep what another program writes during or after a save
- *(design)* check each file again right before the save replaces it
- *(design)* refuse a save over a file changed since the page loaded it
- *(design)* bound the padding a mixed-ending schema's line tags may add
- *(design)* keep each line's own ending in a mixed-ending schema
- *(design)* escape DEL in a value written on one line of a CRLF file
- *(design)* patch a CRLF flusso.toml whose string ends a line with a backslash
- *(design)* keep the spelling of a dotted key toml_edit respells
- *(design)* space a key added to an inline table like the rest
- *(design)* keep the blank line below a top-level dotted key
- *(design)* keep a byte order mark through a save
- *(design)* a json field reads `json` in the document tree; the array switch carries the default
- *(design)* an array default follows the field's element rule, and a save that would not load writes nothing
- *(design)* a blank line after the value slot list ([#276](https://github.com/alias2k/flusso/pull/276))
- review findings on array columns ([#276](https://github.com/alias2k/flusso/pull/276))
- *(design)* recover a save the designer was killed in the middle of
- *(design)* a new sink takes the first free sinkN; state the name rules once
- *(design)* a sink name is applied as flusso reads it
- *(design)* an LF flusso.toml holding U+E00D is patched in place
- *(design)* keep a flusso.toml's line endings and match sink keys as read
- *(design)* a new index takes an unused file only if it reads as a schema; a save keeps file permissions
- *(design)* add an index only with its schema, and say what a failed save keeps
- *(design)* check a save on the server's own pairing, and undo a failed one
- *(design)* keep each index's schema in its own file, and refuse a save that doesn't
- *(design)* a comment ending a file with no final newline stays its table's
- *(design)* keep a renamed sink's comments and a file's last comment
- *(design)* file a waiting option value under its row, not its key
- *(design)* keep focus in an option's key box; select a field by its uid
- *(design)* a removed option stays removed; a value read that never answers fails
- *(design)* apply a typed value to the field it was typed for
- *(design)* keep typed values exact through reset, blur, format and old browsers
- *(design)* a number typed in the inspector is saved as the value it previews
- *(design)* a Code-mode visit never applies the seeded YAML
- *(design)* canvas values and fields equal what their YAML reads back as
- *(design)* write values as plain YAML the parser reads back unchanged
- *(design)* one flush path keeps the Code buffer and the document in step
- *(design)* write a null value as YAML null, not the string 'Null'
- *(design)* Code mode never seeds from the pruned preview YAML
- *(design)* leave a field with a blank or invalid name out of the live preview
- *(design)* badge a finding on a join on the join's own header
- *(design)* match validation findings by path and anchor

### Other

- *(design)* rebuild the SPA on main
- *(design)* probe a request's commit point once per run
- *(design)* a file the user remade after its step ran is kept and named
- *(design)* every change is flushed before the record says it happened
- *(design)* a save file gone after the rollback's plan is never a step done
- *(design)* a kill during recovery, then a user edit, never re-settles a settled file
- *(design)* rebuild the SPA
- *(design)* rebuild the SPA
- *(design)* the press tracker and the rename chips under npm test
- *(design)* a settled save is never settled again; a broken config names nothing outside
- *(design)* a schema outside the folder, an unwritable notices folder, links, wording
- *(design)* a kill during recovery, hand-edited and stale notices, unknown dismiss fields
- *(design)* what the recovery notice says, and what outlives a restart
- *(design)* widen the save property tests to every layout of #279
- *(design)* rebuild the embedded SPA on main; the new save test names when its page loaded
- Merge remote-tracking branch 'origin/main' into fix/check-validation-gaps
- *(design)* move to zustand 5
- *(design)* bump the npm-compatible group (34 updates)

## [0.17.0](https://github.com/alias2k/flusso/compare/flusso-design-v0.16.0...flusso-design-v0.17.0) - 2026-11-12

### Added

- *(design)* script fields and input-only fields in the canvas, the inspector and the sample preview
- *(design)* preview reports script compile errors; the sample runs the scripts on a chosen row
- *(design)* an aggregate's input is a column or an expression, checked by the loader's parser
- *(design)* check an aggregate expression through the loader's parser (POST /api/expression)
- *(schema)* an aggregate's column can be an arithmetic expression
- *(kernel)* script fields and input-only fields in the schema model
- *(design)* author on: pairs on joins, aggregates and via hops
- *(config)* on: column pairs on joins, aggregates and via hops
- *(design)* switch an aggregate's op without losing its table, key, filters or hops
- *(design)* author exists aggregates and via hops
- *(schema)* aggregates across several hops with via, and the exists op
- *(design)* author subfields and indexed opt-outs, show stored-only fields in the preview
- *(kernel)* subfields and indexed opt-outs on columns and joins, hashed only when off
- *(design)* per-sink options on each index

### Fixed

- *(design)* a lone apostrophe in a message renders literally, as in ICU
- review — report script failures under both policies, ids in print context, debug log of the failing document, shared build-by-id, field docs
- *(config)* an expression's sum/min/max needs a numeric value_type
- review findings for aggregate expressions
- *(design)* clear an aggregate's on: pairs when the table their local columns name changes
- review findings for on: pairs
- review findings for via and exists
- *(design)* every retype drops the opt-outs the new type does not take
- *(kernel)* [**breaking**] a map carries its keys' plain-field mapping and a new content hash
- *(design)* drop per-index options when a sink's type takes none
- *(design)* validate per-index sink options before connecting
- review fixes for index settings (owned blocks/hidden, shared schema hoisting)
- *(design)* re-apply persisted node uids by field name, not by path
- *(design)* ignore node uids when deciding whether the doc is unsaved
- *(design)* Code mode adopts node uids by name, duplicated indexes get fresh ones
- *(design)* keep node uids across reloads and migrate before the canvas reads
- *(design)* key canvas collapse and layout on stable node ids

### Other

- say the root manual is the newest release and /main/ tracks main
- Merge remote-tracking branch 'origin/main' into feat/script-fields
- lead with the decimal literal to avoid integer overflow; restore isColumnName's doc comment
- *(design)* rebuild the SPA
- *(design)* the mapping preview shows the mapping every map key gets
- *(design)* rebuild the embedded SPA
- *(design)* rebuild the embedded SPA
- *(design)* rebuild the embedded SPA

## [0.16.0](https://github.com/alias2k/flusso/compare/flusso-design-v0.15.1...flusso-design-v0.16.0) - 2026-10-09

### Added

- *(design)* author distinct and junction filters in the inspector
- *(design)* emit distinct and through.filters in codegen
- *(kernel)* distinct on aggregates and filters on a through junction
- *(design)* author [batch] in the deployment panel; rebuild the SPA
- *(design)* render source, stream, and sink forms from adapter descriptions
- *(config)* [**breaking**] adapters own their configuration; port entries are type + options
- *(design)* TLS keys in the source stage (ssl_mode select + cert/SNI fields), en+it
- *(cli,design)* wire source TLS through both composition roots

### Fixed

- address review — destructured Hash, empty junction filters keep the hash, stale junction filters cleared in the designer
- *(engine)* max_delay caps a burst inside the drain; max_changes is non-zero

### Other

- two engines, per-sink status and reindex, the backfill sink key; designer authors backfill
- describe the kernel / ports / adapters layout and adapter-owned config
- *(arch)* [**breaking**] lay out kernel / ports / adapters / engine / daemon and sdk/
- *(design)* plain list separators in the crate README
- READMEs as landings, repoint every deep link, serve the book at the Pages root, add the docs CI job
- root README as a pitch, flusso-query README as a landing; track the Start here pages
- *(release)* [**breaking**] three release trains — per-crate versions + version groups
- the lock is deterministic TOML; formats frozen for the major
- source TLS — configuration guide section, README, crate README, plugin skills

## [0.14.0](https://github.com/alias2k/flusso/compare/flusso-design-v0.13.0...flusso-design-v0.14.0) - 2026-08-14

### Added

- *(query-derive)* [**breaking**] deprecate path= instead of deleting it

## [0.13.0](https://github.com/alias2k/flusso/compare/flusso-design-v0.12.0...flusso-design-v0.13.0) - 2026-07-24

### Added

- *(design)* author enum variant order in the designer
- *(core)* carry declared enum order on Column + Mapping.enum_order

## [0.12.0](https://github.com/alias2k/flusso/compare/flusso-design-v0.11.1...flusso-design-v0.12.0) - 2026-07-24

### Added

- *(design)* reusable Table/Column pickers in the inspector
- *(design)* multi-select column rows with a bulk include/remove panel
- *(design)* Shift-click to check/uncheck a range of node columns
- *(design)* directory picker for the New-index schema path
- *(design)* searchable root-table picker with junction icons
- *(design)* remember the minimap open state per index
- *(design)* command-palette fixes + path-based routing
- *(design)* reorganize canvas controls into view / node clusters
- *(design)* client op set + folder-tree save review
- *(design)* stateless op-based save (upsert/move/delete)
- *(design)* card UX polish + schema file placement at creation

### Fixed

- *(design)* make the topbar search shrink instead of overflow
- *(design)* center the topbar search, let the right cluster push it
- *(design)* take the topbar search out of absolute centering
- *(design)* hover + pointer on excluded column rows
- *(design)* row multi-select now spans custom fields & excluded columns
- *(design)* make column range-select actually work + add Ctrl/Cmd
- *(design)* directory picker on a relative config path + picker polish
- *(design)* restore the saved viewport on load, not just on index switch
- *(design)* combobox scrolls inside dialogs; keep view controls off the RF attribution
- *(design)* show the minimap top-left under its toggle, not top-right

### Other

- *(design)* rebuild embedded SPA
- *(design)* rebuild embedded SPA
- *(design)* rebuild embedded SPA
- *(design)* rebuild embedded SPA
- *(design)* rebuild embedded SPA

## [0.11.1](https://github.com/alias2k/flusso/compare/flusso-design-v0.11.0...flusso-design-v0.11.1) - 2026-07-23

### Fixed

- *(design)* drop dead RelationSuggestion.label and fix node height estimate
- *(design)* restore the focus ring on the + field trigger
- *(design)* bound FK suggestions behind a searchable, grouped picker
- *(design)* theme popover and select dropdown borders

### Other

- Merge branch 'main' into fix/designer-relation-suggestions-overflow

## [0.11.0](https://github.com/alias2k/flusso/compare/flusso-design-v0.10.1...flusso-design-v0.11.0) - 2026-07-23

### Added

- *(design)* single-save Code flow, global mode toggle, Tables view, hash router
- *(design)* Code mode — a real YAML editor with live problems
- *(design)* structured /api/parse endpoint for the Code editor
- *(design)* split settings buttons, animate node collapse, QoL sweep
- *(design)* readable preview document tree + selectable drawer text
- *(design)* rebuild the Deployment panel as pipeline stages
- *(design)* add toggle-all shortcut to the save review
- *(design)* full keyboard control + shortcut legend in the save review
- *(design)* focus the file filter when the save review opens
- *(design)* keybindings for the save review's file search
- *(design)* filter the save review's file list
- *(design)* choose which files to save in the review
- *(design)* run quick validation in the save review
- *(design)* diff uses solid color shades; gap expander polish
- *(design)* master-detail save review with a file list
- *(design)* inline word-diff for small edits in the unified view
- *(design)* word-level highlighting in the split diff too
- *(design)* word-level highlighting in the unified diff; split toggle first
- *(design)* side-by-side split view in the save diff
- *(design)* Old/New/Unified view toggle in the save diff
- *(design)* collapsible diff files with clearer separation
- *(design)* git-style line-level diff in the save review
- *(design)* tooltips on every header button
- *(design)* split the header into a global bar and a per-index context bar
- *(design)* keep a fetched sample across tab switches
- *(design)* sample refresh in the code corner; corner tooltips on top
- *(design)* proper empty state for the Sample tab
- *(design)* pin the code-copy button to the block corner
- *(design)* syntax-highlight the preview YAML/JSON tabs
- *(design)* modal preview drawer + guided document tree
- *(design)* overhaul schema preview into a tabbed right drawer
- *(design)* guarantee plural name suggestions for to-many joins
- *(design)* master select-all checkbox for node columns
- *(design)* make node all/none control icon-only with Hint tooltips
- *(design)* segmented all/none control on canvas node column filter
- *(design)* scroll the jumped-to table into view in the catalog list
- *(design)* make catalog FK references navigable
- *(design)* align catalog columns in a true grid, widen dialog
- *(design)* master-detail database browser with inline FKs & column filter
- *(design)* add shadcn Kbd atom, use it for all palette shortcuts
- *(design)* palette autocomplete, recent searches & smarter ranking
- *(design)* MiniSearch-powered palette with fuzzy, boosting & frecency
- *(design)* redesign command palette — typed rows + live preview pane
- *(design)* global command palette (⌘K) search
- *(design)* ad-hoc node search, fitView jump, expand-on-jump
- *(design)* rebuild node search on the shared Combobox
- *(design)* legend descriptions inline instead of on hover
- *(design)* describe each legend swatch on hover, clearer headings
- *(design)* friendlier value_op filter layout + multi-value inputs
- *(design)* dev-lingo filter operators with descriptions
- *(design)* translate the filter kind labels
- *(design)* replace React Flow Controls with shadcn icon buttons
- *(design)* collapsible legend pinned below a scrolling index list
- *(design)* regroup topbar — icons, separators, and a More menu
- *(design)* translate diagnostics + fix-all/ignore banner for type mismatches
- *(design)* flag a drastic source→type change on the canvas + tighten the rule
- *(design)* warn on a drastic source→document type change
- *(design)* tolerant preview + searchable column combobox
- *(design)* add a Reset button to discard unsaved changes
- *(design)* colour-code every kind in the add menus
- *(design)* describe + colour-code the + field / + join kind menus
- *(design)* colour-code + describe each option in the TYPE dropdown
- *(design)* field-type colour legend + align type colours in the inspector
- *(design)* rebuild the field inspector as C1 — source ⟷ document blocks
- *(design)* band the field inspector into Identity / Source / Mapping sections
- *(design)* show required/default state per column on the canvas
- *(design)* localize the designer UI (English + Italian)
- *(design)* nudge the field type toward the source suggestion
- *(design)* show a belongs_to join's FK-derived optionality in the inspector
- *(design)* seed belongs_to optionality from its FK column
- *(design)* source-guided required/default rule
- *(design)* synthesize example data when the sample table is empty
- *(design)* sample-document preview from a live row
- *(design)* catalog/ER browser
- *(design)* edit env-ref and parts-form connections
- *(design)* remember pan/zoom per index across switches
- *(design)* duplicate a node, field, or index
- *(design)* save only the files that actually change
- *(design)* structural completeness hints + reset-layout
- *(design)* more config + the OpenSearch mapping view
- *(design)* drop manual column editing; add field default + options
- *(design)* polish — light theme, copy YAML, a11y (DX group G)
- *(design)* config editing & escape hatches (DX group F)
- *(design)* onboarding & discoverability (DX group E)
- *(design)* canvas navigation (DX group D)
- *(design)* column ergonomics (DX group C)
- *(design)* feedback & validation surfacing (DX group B)
- *(design)* undo/redo + unsaved-changes tracking (DX group A)
- *(design)* distinct colour per relation kind; root is orange
- *(design)* cohesive 'flow' visual pass
- *(design)* hide the minimap behind a toggle button
- *(design)* install stderr logging so the listening URL prints
- *(design)* node rows are display-only; move field editing to the inspector
- *(design)* collapsible sidebars; auto-hide inspector when nothing is selected
- *(design)* node-graph canvas (React Flow) replacing the form editor
- *(design)* open the designer in a browser on start
- *(design)* React SPA + flusso design CLI subcommand
- *(design)* add flusso-design crate — server, codegen, preview, API
- improve upon claude skills
- add the publication management
- change the "run" command to follow the "cargo" pattern of updating the "lock" file
- license, security, coc, contributing and github templates
- rename flusso-search to flusso-query
- move files to correctly reflect dependencies
- create alias pointing to the latest index
- add belong_to
- start client, add geo
- rename config.toml to flusso.toml and flusso.bin to flusso.lock
- add compile functionality
- better opensearch defaults
- improve env vars handling and readme about sinks and sources
- backfill and renaming
- rebranding to storno
- update deps
- documentation

### Fixed

- *(design)* size the Combobox popover to its content
- *(design)* visual polish from the UI audit
- *(design)* polish — picker auto-select, styled remove confirm, junction hints
- *(design)* Preview in Code mode, rename schema path, revert active
- *(design)* disable Save when there is nothing to save
- *(design)* Reset restores the last-saved state
- *(design)* Esc no longer closes the save review
- *(design)* always show split scrollbars so halves stay aligned
- *(design)* keep split diff halves at a fixed 50/50
- *(design)* pin the diff horizontal scrollbar to the pane bottom
- *(design)* pointer cursor on the diff file-list rows
- *(design)* keep block grouping + guard word-diff to similar lines
- *(design)* default diff to split, fix unified line numbers, add view icons
- *(design)* Preview button always opens the drawer, no Hide toggle
- *(design)* anchor the code-copy tooltip to the button
- *(design)* singularizer mishandled -es plurals
- *(design)* let wide dialogs actually widen past shadcn's sm:max-w-lg
- *(design)* stop password managers hijacking text/filter inputs
- *(design)* Spotlight-style palette ranking + kill the white dialog border
- *(design)* align command palette with the approved design
- *(design)* drive legend row bg off the tooltip open state
- *(design)* keep legend row hovered while its tooltip shows
- *(design)* legend tooltip opens on whole-row hover
- *(design)* shrink legend hover target to its text
- *(design)* legend tooltip to the right, not above
- *(design)* keep the first IN/NOT IN value undeletable
- *(design)* correct filter wire shape + shared coloured ColumnPicker
- *(design)* filter column uses the searchable Combobox
- *(design)* lay out filter rows as a card, not a cramped line
- *(design)* center node connection handles on the node edge
- *(design)* canvas panel buttons are square shadcn icon buttons
- *(design)* Fix-all button is actually amber (no brand gradient)
- *(design)* float the type-mismatch banner over the canvas
- *(design)* node grows to fit columns, no inner scroll
- *(design)* order_by column is a Select with a valid default
- *(design)* stable datalist id so column pickers actually work
- *(design)* soft-delete value, order_by layout, canvas button styling
- *(design)* nudge dots outward, theme list scrollbar, align card X
- *(design)* connection dots — layer RF css so our handle styles win
- *(design)* stop legacy button chrome inflating shadcn atoms
- *(design)* checkbox rounded-[4px] → rounded-sm token
- *(design)* checkbox to size-3 + drop dead raw-input CSS
- *(design)* shrink checkbox atom — size-4 was too big
- *(design)* default only when it matters + 'from source' on aligned required
- *(design)* smaller, centered checkbox tick + add hot-reload dev recipe
- *(design)* drawer class collision, aligned actions, crisp checkbox
- *(design)* stack the field-name label, uppercase field labels
- *(design)* custom checkbox + elevated row selection
- *(design)* options/default/constant speak GenericValue, explain + contain them
- *(design)* readable field selection, source-column facts, no dead default input
- *(design)* mark required columns with an asterisk, not an ambiguous dot
- *(design)* pull belongs_to into the blue family, separate green joins, tint selection by kind
- *(design)* widen the spacing between relation hues
- *(design)* recolor kind legend into root / object / relation families
- *(design)* theme type-less text inputs (filter boxes, manual column)
- *(design)* clip node corners; drop field reordering
- *(design)* unknown column is a diagnostic, not 'database not reachable' + theme the search bar
- *(design)* stable icon buttons + hover tooltips, translate the canvas
- *(design)* distinguish empty-table from no-PK in sample preview
- *(design)* index rename loses schema; test-connection used on-disk config
- *(design)* allow test-only dev-deps + graceful shutdown; add coverage recipe
- *(design)* re-tidy on measured-height changes, not a one-shot catalog flag
- *(design)* tighten node spacing — stop the estimate from clobbering the measured layout
- *(design)* pin grid columns + relayout from measured node heights
- *(design)* variable-height auto-layout so nodes don't overlap on launch
- *(design)* theme React Flow controls so icons aren't invisible
- *(design)* guard tree walks against stale paths (crash on node delete)
- *(design)* stop the type-chip select from being clipped at the node edge

### Other

- *(design)* rebuild the embedded SPA
- *(design)* rebuild the embedded SPA
- *(design)* remove the Playwright browser e2e suite
- update packages
- *(design)* globalise Tables, inset the context bar, icon-only Settings
- *(design)* use pluralize for name singularization
- *(design)* fix stale Fuse reference and trim frecency header
- *(design)* full-row legend hover bg, drop the help cursor
- *(design)* hover state on legend rows
- *(design)* strip narrating comments from this session's code
- *(design)* legend descriptions back in a tidy hover tooltip
- *(design)* app-wide themed scrollbars + edge-to-edge legend
- *(design)* shared AddButton/RemoveButton across inspector + config
- *(design)* pointer cursor on all buttons
- *(design)* delete-first order, always-red trash in inspector header
- *(design)* tuck inspector header actions, icon-ify order_by row
- *(design)* inspector actions as header icon buttons
- *(design)* move editor state into zustand stores
- consolidate per-project .gitignore into a single root file
- *(design)* right-align type-mismatch banner, amber Fix-all button
- *(design)* apply Prettier across the frontend
- *(design)* add Prettier — config, scripts, ESLint + CI + just wiring
- *(design)* wrap bespoke CSS in @layer components, fine-tune dot offsets
- *(design)* headings → <PanelTitle>/<SectionTitle> molecules
- *(design)* remove global <button> chrome entirely
- *(design)* retire app-shell layout-grid bespoke CSS (retirement 12/n)
- *(design)* retire leaf .loading/.crumbs/.error-hint CSS (retirement 11/n)
- *(design)* retire Preview document-tree/diagnostics bespoke CSS (retirement 10/n)
- *(design)* retire catalog-browser + diff-view bespoke CSS (retirement 9/n)
- *(design)* retire ConfigPanel/Filters/inspector-editor bespoke CSS (retirement 8/n)
- *(design)* production-grade type-aware lints (typescript-eslint + react + a11y)
- *(design)* upgrade to latest — React 19, TS 6, ESLint 10, vite 8, plugin-react 6
- *(design)* preview bottom-sheet → shadcn Drawer (vaul)
- *(design)* add ESLint (flat config) wired into npm, CI, and just
- *(design)* move remaining raw selects/checkbox/textarea onto shadcn (retirement 7/n)
- *(design)* drop dead col-row rename/select CSS
- *(design)* flusso palette + sizes as Tailwind @theme tokens (retirement 6/n)
- *(design)* topbar + banners to Tailwind, drop dead tooltip CSS (retirement 5/n)
- *(design)* sidebar + new-index to Tailwind utilities (retirement 4/n)
- *(design)* retire styles.css into index.css components layer (retirement 3/n)
- *(design)* inspector one-offs to Tailwind utilities (retirement 2/n)
- *(design)* inspector Block/Bridge/Drawer molecules to Tailwind (retirement 1/n)
- *(design)* CatalogBrowser on shadcn Dialog; drop dead modal CSS (phase 2)
- *(design)* hover hints on Radix Tooltip via a Hint molecule (phase 2)
- *(design)* diff modal on shadcn Dialog + reduced-motion (phase 2)
- *(design)* Select + Checkbox on Radix (shadcn), rewrite affected e2e (phase 2)
- *(design)* text inputs on shadcn Input + bind dark variant (phase 2)
- *(design)* topbar + raw actions on shadcn Button (phase 2)
- *(design)* generate shadcn atoms (phase 1)
- *(design)* add Tailwind v4 + shadcn foundation (phase 0)
- *(design)* migrate the stylesheet from magic px to rem
- *(design)* route every text input through the Text widget
- *(design)* theme React Flow via its --xy-* variables, drop redundant overrides
- make designer + translation alignment a CI-enforced rule
- *(design)* namespaced i18n keys + ICU MessageFormat
- *(cli)* put the designer behind a default-on feature; Docker omits it
- *(design)* Playwright UI e2e + save→flusso-check pipeline + CI job
- *(design)* property-based codegen round-trip (the 'fuzz' layer)
- *(design)* build the SPA + guard against committed-dist drift
- document the visual schema designer
- lead README with the tagline, move AI disclosure below it
- consistency pass — fix factual errors and terminology drift
- rewrite all docs to the prose style guide
- clarify pre-commit hook formats the whole workspace
- add dev workflow tooling
- split into an mdBook manual + per-crate READMEs
- Merge pull request #9 from alias2k/feature/run-command-rewamp
- preparation for going public
- documentation
- add requirements section
- add SCHEMA.md
- readme
- readme and cleanup
- README
