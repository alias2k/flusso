// Italian catalog: English source string → Italian. Missing keys fall back to
// the English source, so a partial translation degrades gracefully.
export const it: Record<string, string> = {
  // topbar
  "Hide sidebar": "Nascondi pannello",
  "Show sidebar": "Mostra pannello",
  "Browse the database": "Esplora il database",
  Tables: "Tabelle",
  Visual: "Visuale",
  "Raw YAML": "YAML grezzo",
  "Undo (⌘Z)": "Annulla (⌘Z)",
  "Redo (⇧⌘Z)": "Ripeti (⇧⌘Z)",
  "Toggle theme": "Cambia tema",
  "Toggle light/dark theme": "Cambia tema chiaro/scuro",
  "Re-test the database connection": "Riprova la connessione al database",
  "DB connected": "DB connesso",
  "DB offline": "DB offline",
  "Database not reachable — offline authoring only.": "Database non raggiungibile — solo modifica offline.",
  Hide: "Nascondi",
  YAML: "YAML",
  Validate: "Convalida",
  Save: "Salva",
  "Unsaved changes": "Modifiche non salvate",
  "Up to date": "Aggiornato",
  Language: "Lingua",

  // toasts / status
  "Saved {n} file(s)": "Salvati {n} file",
  "Save failed: {err}": "Salvataggio non riuscito: {err}",
  "Saved raw YAML": "YAML grezzo salvato",
  "Validate failed: {err}": "Convalida non riuscita: {err}",
  "{n} issue(s) — see the highlighted fields": "{n} problema/i — vedi i campi evidenziati",
  "Database not reachable": "Database non raggiungibile",
  "Database connected": "Database connesso",
  "Already up to date": "Già aggiornato",
  "Diff failed: {err}": "Confronto non riuscito: {err}",
  "Database not reachable: {err}": "Database non raggiungibile: {err}",
  "Schemas match the database": "Gli schemi corrispondono al database",

  // sidebar
  Deployment: "Distribuzione",
  Indexes: "Indici",
  "(off)": "(disattivo)",
  Kinds: "Tipi",
  "New index": "Nuovo indice",
  "index name": "nome indice",
  "root table": "tabella radice",
  Create: "Crea",

  // raw-YAML mode
  "Editing raw YAML for": "Modifica YAML grezzo di",
  "Save raw writes this file verbatim, then reloads.": "Salva grezzo scrive questo file così com'è e ricarica.",
  "Save raw": "Salva grezzo",
  Cancel: "Annulla",

  // diff modal
  "Review changes": "Rivedi le modifiche",
  "Review changes ({n} file(s))": "Rivedi le modifiche ({n} file)",
  "(new file)": "(nuovo file)",
  "Write {n} file(s)": "Scrivi {n} file",

  // inspector — common
  "Select a node or field to edit its details.": "Seleziona un nodo o un campo per modificarne i dettagli.",
  "Select or edit an index to preview it.": "Seleziona o modifica un indice per visualizzarne l'anteprima.",
  Duplicate: "Duplica",
  Delete: "Elimina",
  "field name": "nome campo",
  required: "obbligatorio",
  type: "tipo",
  use: "usa",
  "default (required)": "valore predefinito (obbligatorio)",
  "default (optional, JSON)": "valore predefinito (facoltativo, JSON)",
  lowercase: "minuscolo",
  trim: "ritaglia",
  "Index root": "Radice dell'indice",
  schema: "schema",
  "root filters": "filtri di radice",
  "Object group": "Gruppo oggetto",
  "A group nests columns of the same table. Add fields on its node.":
    "Un gruppo annida colonne della stessa tabella. Aggiungi campi sul suo nodo.",
  Join: "Relazione",
  Field: "Campo",
  verb: "verbo",
  table: "tabella",
  "column (this table → target)": "colonna (questa tabella → destinazione)",
  "foreign_key (on target)": "foreign_key (sulla destinazione)",
  "FK column {col} is nullable — the target may be absent, so this join is optional.":
    "La colonna FK {col} ammette null — la destinazione può mancare, quindi questa relazione è facoltativa.",
  "FK column {col} is NOT NULL — the target is always present.":
    "La colonna FK {col} è NOT NULL — la destinazione è sempre presente.",
  filters: "filtri",
  column: "colonna",
  "value (JSON)": "valore (JSON)",
  "Source column is {sql} — suggests {ty}.": "La colonna di origine è {sql} — suggerisce {ty}.",
  "Source column is NOT NULL — required by default; uncheck to make it optional in the document.":
    "La colonna di origine è NOT NULL — obbligatoria per impostazione predefinita; deseleziona per renderla facoltativa nel documento.",
  "Source column is nullable — optional by default; to make it required you must set a default.":
    "La colonna di origine ammette null — facoltativa per impostazione predefinita; per renderla obbligatoria devi impostare un valore predefinito.",
  "A required field over a nullable column must set a default, or the document field could be missing.":
    "Un campo obbligatorio su una colonna che ammette null deve impostare un valore predefinito, altrimenti il campo del documento potrebbe mancare.",
  "options ({n})": "opzioni ({n})",
  key: "chiave",
  value: "valore",
  option: "opzione",
  values: "valori",
  "column (json/jsonb)": "colonna (json/jsonb)",
  "postgres types (comma)": "tipi postgres (virgola)",
  "opensearch type": "tipo opensearch",
  "lat column": "colonna lat",
  "lon column": "colonna lon",
  "A point is absent when either column is null (never sent as {lat:null}). Mark required to forbid that absence.":
    "Un punto è assente quando una delle colonne è null (mai inviato come {lat:null}). Segna obbligatorio per vietare tale assenza.",
  "related table": "tabella correlata",
  "column (to aggregate)": "colonna (da aggregare)",
  "junction table": "tabella di giunzione",
  "soft delete": "eliminazione logica",
  "document field": "campo del documento",
  // KIND_HELP grammar hints
  "This row points at one target row (single nested object).":
    "Questa riga punta a una riga di destinazione (un oggetto annidato).",
  "One related row points back here (single nested object).":
    "Una riga correlata punta qui (un oggetto annidato).",
  "Many related rows point back here (array of objects).":
    "Molte righe correlate puntano qui (array di oggetti).",
  "Related rows linked through a junction table (array of objects).":
    "Righe correlate collegate tramite una tabella di giunzione (array di oggetti).",
  "Groups columns of the same table under a nested object — no new table.":
    "Raggruppa colonne della stessa tabella in un oggetto annidato — nessuna nuova tabella.",
  "Counts the related rows into a number.": "Conta le righe correlate in un numero.",
  "Sums a column of the related rows.": "Somma una colonna delle righe correlate.",
  "Averages a column of the related rows (result is a double).":
    "Calcola la media di una colonna delle righe correlate (il risultato è un double).",
  "Smallest value of a column across the related rows.":
    "Il valore minimo di una colonna tra le righe correlate.",
  "Largest value of a column across the related rows.":
    "Il valore massimo di una colonna tra le righe correlate.",
  "Collects the related table's primary keys into an array.":
    "Raccoglie le chiavi primarie della tabella correlata in un array.",
  "A geo point from two columns (lat/lon).": "Un punto geografico da due colonne (lat/lon).",
  "A dynamic-key object over a json/jsonb column; keys stay searchable.":
    "Un oggetto a chiavi dinamiche su una colonna json/jsonb; le chiavi restano ricercabili.",
  "A type flusso doesn't model — you give the Postgres + OpenSearch types.":
    "Un tipo che flusso non modella — indichi tu i tipi Postgres + OpenSearch.",
  "A fixed value baked into every document.": "Un valore fisso incluso in ogni documento.",

  // preview
  Document: "Documento",
  "Database check": "Controllo del database",
  "Sample from DB": "Esempio dal DB",
  fetch: "ottieni",
  refresh: "aggiorna",
  "building…": "costruzione…",
  example: "esempio",
  "OpenSearch mapping": "Mappatura OpenSearch",
  copy: "copia",
  copied: "copiato",
  Close: "Chiudi",
  "This schema does not parse:": "Questo schema non è analizzabile:",
  "schema.yml": "schema.yml",
  "Copy YAML": "Copia YAML",
  "the root table has no rows — showing example data from the schema":
    "la tabella radice non ha righe — mostro dati di esempio dallo schema",
  "this index has no single-column primary key, so a row can't be sampled":
    "questo indice non ha una chiave primaria a colonna singola, quindi non è possibile campionare una riga",

  // catalog browser
  "Database tables": "Tabelle del database",
  "Database ({n} tables)": "Database ({n} tabelle)",
  "Database not reachable — {err}": "Database non raggiungibile — {err}",
  "filter tables…": "filtra tabelle…",
  junction: "giunzione",
  "{n} cols": "{n} colonne",
  "primary key": "chiave primaria",

  // canvas node (DocNodeView)
  Expand: "Espandi",
  Collapse: "Comprimi",
  remove: "rimuovi",
  pk: "cp",
  "{n} fields": "{n} campi",
  "{n} field(s) disagree with the database": "{n} campo/i non concordano con il database",
  "This join is missing a required key — set it in the inspector":
    "A questa relazione manca una chiave obbligatoria — impostala nell'ispettore",
  "Pick a root table to begin": "Scegli una tabella radice per iniziare",
  "choose a table…": "scegli una tabella…",
  "root table name, Enter": "nome tabella radice, Invio",
  "filter columns…": "filtra colonne…",
  "include all columns": "includi tutte le colonne",
  all: "tutte",
  "clear all columns": "rimuovi tutte le colonne",
  none: "nessuna",
  "column: {name}": "colonna: {name}",
  "incomplete — set its key/column in the inspector":
    "incompleto — imposta la sua chiave/colonna nell'ispettore",
  "+ join": "+ relazione",
  "+ field": "+ campo",
  "move up": "sposta su",
  "move down": "sposta giù",
  "+ column name, Enter": "+ nome colonna, Invio",

  // config panel
  "index prefix": "prefisso indice",
  "(none)": "(nessuno)",
  Sinks: "Destinazioni",
  sink: "destinazione",
  name: "nome",
  enabled: "abilitato",
  duplicate: "duplica",
  dup: "dup",
  index: "indice",
  'Remove index "{name}"? (the schema file is left on disk)':
    'Rimuovere l\'indice "{name}"? (il file schema resta su disco)',
  connection: "connessione",
  "env var": "variabile env",
  host: "host",
  port: "porta",
  user: "utente",
  password: "password",
  database: "database",

  // filters
  filter: "filtro",
  "lo, hi": "min, max",
  "a, b, c": "a, b, c",
};
