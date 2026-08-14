//! Resumable Anki collection-package imports.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chrono::{DateTime, FixedOffset, Utc};
use prost::Message;
use regex::{Captures, Regex};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::{Builder, NamedTempFile};
use zip::ZipArchive;

use crate::document::{CardEvent, render_card_history};

type Reviews = HashMap<i64, Vec<CardEvent>>;
type ImportErrors = Vec<String>;

static TEMPLATE_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*([^{}]+?)\s*\}\}").expect("valid template regex"));
static CLOZE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\{\{c([0-9]+)::(.*?)(?:::(.*?))?\}\}").expect("valid cloze regex")
});
static ANSWER_SEPARATOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<hr\b[^>]*>").expect("valid answer separator regex"));
static SOUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[sound:([^\]\r\n]+)\]").expect("valid sound regex"));
static SCRIPT_OR_STYLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(?:script|style)\b[^>]*>.*?</(?:script|style)\s*>")
        .expect("valid script/style regex")
});
static LEFTOVER_HTML: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<!--.*?-->|<!doctype\b[^>]*>|</?[a-z][a-z0-9-]*(?:\s[^>]*)?/?>")
        .expect("valid HTML regex")
});

/// Completed, skipped, and failed work from one resumable import pass.
#[derive(Debug, Default)]
pub struct ImportReport {
    pub imported_cards: usize,
    pub skipped_cards: usize,
    pub imported_media: usize,
    pub skipped_media: usize,
    pub events: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug)]
struct Collection {
    decks: HashMap<i64, String>,
    models: HashMap<i64, Model>,
    cards: Vec<AnkiCard>,
    reviews: Reviews,
}

#[derive(Debug, Default)]
struct Model {
    fields: Vec<String>,
    templates: Vec<Template>,
}

#[derive(Debug, Default)]
struct Template {
    ordinal: usize,
    question: String,
    answer: String,
}

#[derive(Clone, PartialEq, Message)]
struct AnkiTemplateConfig {
    #[prost(string, tag = "1")]
    question: String,
    #[prost(string, tag = "2")]
    answer: String,
}

#[derive(Clone, PartialEq, Message)]
struct AnkiMediaEntries {
    #[prost(message, repeated, tag = "1")]
    entries: Vec<AnkiMediaEntry>,
}

#[derive(Clone, PartialEq, Message)]
struct AnkiMediaEntry {
    #[prost(string, tag = "1")]
    name: String,
    #[prost(uint32, tag = "2")]
    _size: u32,
    #[prost(bytes = "vec", tag = "3")]
    _sha1: Vec<u8>,
    #[prost(uint32, optional, tag = "255")]
    legacy_zip_filename: Option<u32>,
}

#[derive(Debug)]
struct AnkiCard {
    id: i64,
    deck_id: i64,
    ordinal: usize,
    model_id: i64,
    fields: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreateOutcome {
    Created,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageFormat {
    Legacy,
    Latest,
}

/// Default to a sibling directory whose name is the archive stem.
pub fn default_output_path(archive: &Path) -> Result<PathBuf, String> {
    let stem = archive.file_stem().ok_or_else(|| {
        format!(
            "{}: cannot derive an output directory; pass --output",
            archive.display()
        )
    })?;
    if stem.is_empty() {
        return Err(format!(
            "{}: cannot derive an output directory; pass --output",
            archive.display()
        ));
    }
    Ok(archive.parent().unwrap_or_else(|| Path::new("")).join(stem))
}

/// Import all cards and media, continuing past item-level failures.
pub fn import(archive_path: &Path, output: &Path) -> Result<ImportReport, String> {
    validate_archive(archive_path)?;
    let file = File::open(archive_path)
        .map_err(|error| format!("{}: cannot open archive: {error}", archive_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("{}: invalid .colpkg ZIP: {error}", archive_path.display()))?;
    let (database, format) = extract_database(&mut archive, archive_path)?;
    let (collection, mut database_errors) = read_collection(database.path())?;
    let media = read_media_manifest(&mut archive, archive_path, format)?;

    prepare_directory(output, "output vault")?;
    let images = output.join("images");
    prepare_directory(&images, "media directory")?;

    let mut report = ImportReport::default();
    report.errors.append(&mut database_errors);
    let media_paths = import_media(&mut archive, &media, &images, format, &mut report);

    for card in &collection.cards {
        let filename = format!("{}.md", stable_name("card", &card.id.to_string()));
        let path = output.join(&filename);
        let source = render_card(card, &collection, &media_paths);
        match atomic_create(&path, source.as_bytes()) {
            Ok(CreateOutcome::Created) => {
                report.imported_cards += 1;
                report.events.push(format!("created card {filename}"));
            }
            Ok(CreateOutcome::Skipped) => {
                report.skipped_cards += 1;
                report
                    .events
                    .push(format!("skipped card {filename}: already exists"));
            }
            Err(error) => report.errors.push(error),
        }
    }

    Ok(report)
}

fn validate_archive(path: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{}: symbolic links are not accepted",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "{}: expected a regular .colpkg file",
            path.display()
        ));
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("colpkg"))
    {
        return Err(format!("{}: expected a .colpkg file", path.display()));
    }
    Ok(())
}

fn prepare_directory(path: &Path, kind: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "{}: {kind} cannot be a symbolic link",
                path.display()
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!("{}: {kind} must be a directory", path.display()));
        }
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "{}: cannot inspect {kind}: {error}",
                path.display()
            ));
        }
    }
    fs::create_dir_all(path)
        .map_err(|error| format!("{}: cannot create {kind}: {error}", path.display()))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{}: cannot inspect created {kind}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{}: created {kind} is not a real directory",
            path.display()
        ));
    }
    Ok(())
}

fn extract_database(
    archive: &mut ZipArchive<File>,
    archive_path: &Path,
) -> Result<(NamedTempFile, PackageFormat), String> {
    let name = [
        "collection.anki21b",
        "collection.21b",
        "collection.anki21",
        "collection.anki2",
    ]
    .into_iter()
    .find(|name| archive.file_names().any(|candidate| candidate == *name))
    .ok_or_else(|| {
        format!(
            "{}: package contains no collection.anki21 or collection.anki2 database",
            archive_path.display()
        )
    })?;

    let entry = archive.by_name(name).map_err(|error| {
        format!(
            "{}: cannot read database entry {name}: {error}",
            archive_path.display()
        )
    })?;
    let mut reader = BufReader::new(entry);
    let prefix = reader
        .fill_buf()
        .map_err(|error| format!("{}: cannot read {name}: {error}", archive_path.display()))?;
    let format = if name.ends_with("21b") {
        PackageFormat::Latest
    } else {
        PackageFormat::Legacy
    };
    let compressed =
        format == PackageFormat::Latest || prefix.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]);
    let mut temporary = Builder::new()
        .prefix("retent-anki-")
        .suffix(".sqlite")
        .tempfile()
        .map_err(|error| format!("cannot create temporary Anki database: {error}"))?;
    if compressed {
        zstd::stream::copy_decode(reader, temporary.as_file_mut()).map_err(|error| {
            format!(
                "{}: cannot decompress database entry {name}: {error}",
                archive_path.display()
            )
        })?;
    } else {
        std::io::copy(&mut reader, temporary.as_file_mut()).map_err(|error| {
            format!(
                "{}: cannot extract database entry {name}: {error}",
                archive_path.display()
            )
        })?;
    }
    temporary
        .as_file_mut()
        .flush()
        .map_err(|error| format!("cannot flush temporary Anki database: {error}"))?;
    Ok((temporary, format))
}

fn read_collection(path: &Path) -> Result<(Collection, Vec<String>), String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("Anki collection database cannot be opened: {error}"))?;
    let (models_json, decks_json, conf_json): (String, String, String) = connection
        .query_row("SELECT models, decks, conf FROM col LIMIT 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|error| format!("Anki collection metadata cannot be read: {error}"))?;
    let models = read_models(&connection, &models_json)?;
    let decks = read_decks(&connection, &decks_json)?;
    let offset = collection_offset(&connection, &conf_json);
    let (reviews, errors) = read_reviews(&connection, offset)?;
    let cards = read_cards(&connection)?;
    Ok((
        Collection {
            decks,
            models,
            cards,
            reviews,
        },
        errors,
    ))
}

fn parse_models(source: &str) -> Result<HashMap<i64, Model>, String> {
    let values: Value = serde_json::from_str(source)
        .map_err(|error| format!("Anki note types are invalid JSON: {error}"))?;
    let object = values
        .as_object()
        .ok_or_else(|| "Anki note types JSON is not an object".to_owned())?;
    let mut models = HashMap::new();
    for (key, value) in object {
        let Some(id) = key
            .parse::<i64>()
            .ok()
            .or_else(|| value.get("id")?.as_i64())
        else {
            continue;
        };
        let fields = value
            .get("flds")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|field| field.get("name").and_then(Value::as_str))
            .map(str::to_owned)
            .collect();
        let templates = value
            .get("tmpls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|template| Template {
                ordinal: template
                    .get("ord")
                    .and_then(Value::as_u64)
                    .and_then(|ordinal| usize::try_from(ordinal).ok())
                    .unwrap_or(0),
                question: template
                    .get("qfmt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                answer: template
                    .get("afmt")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
            .collect();
        models.insert(id, Model { fields, templates });
    }
    Ok(models)
}

fn read_models(connection: &Connection, legacy_json: &str) -> Result<HashMap<i64, Model>, String> {
    let mut models = parse_models(legacy_json)?;
    let Ok(mut field_statement) =
        connection.prepare("SELECT ntid, ord, name FROM fields ORDER BY ntid, ord")
    else {
        return Ok(models);
    };
    let fields = field_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("Anki note fields cannot be queried: {error}"))?;
    for field in fields {
        let (notetype_id, ordinal, name) =
            field.map_err(|error| format!("Anki note field cannot be read: {error}"))?;
        let model = models.entry(notetype_id).or_default();
        let ordinal = usize::try_from(ordinal).unwrap_or(model.fields.len());
        if model.fields.len() <= ordinal {
            model.fields.resize(ordinal + 1, String::new());
        }
        model.fields[ordinal] = name;
    }

    let mut template_statement = connection
        .prepare("SELECT ntid, ord, config FROM templates ORDER BY ntid, ord")
        .map_err(|error| format!("Anki card templates cannot be queried: {error}"))?;
    let templates = template_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|error| format!("Anki card templates cannot be read: {error}"))?;
    for template in templates {
        let (notetype_id, ordinal, config) =
            template.map_err(|error| format!("Anki card template cannot be read: {error}"))?;
        let config = AnkiTemplateConfig::decode(config.as_slice()).map_err(|error| {
            format!("Anki card template {notetype_id}/{ordinal} has invalid configuration: {error}")
        })?;
        models
            .entry(notetype_id)
            .or_default()
            .templates
            .push(Template {
                ordinal: usize::try_from(ordinal).unwrap_or(0),
                question: config.question,
                answer: config.answer,
            });
    }
    Ok(models)
}

fn read_decks(connection: &Connection, legacy_json: &str) -> Result<HashMap<i64, String>, String> {
    if let Ok(mut statement) = connection.prepare("SELECT id, name FROM decks") {
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| format!("Anki decks cannot be queried: {error}"))?;
        let mut decks = HashMap::new();
        for row in rows {
            let (id, name) = row.map_err(|error| format!("Anki deck cannot be read: {error}"))?;
            decks.insert(id, name);
        }
        if !decks.is_empty() {
            return Ok(decks);
        }
    }

    let values: Value = serde_json::from_str(legacy_json)
        .map_err(|error| format!("Anki decks are invalid JSON: {error}"))?;
    let mut decks = HashMap::new();
    for (key, value) in values.as_object().into_iter().flatten() {
        if let (Ok(id), Some(name)) = (
            key.parse::<i64>(),
            value.get("name").and_then(Value::as_str),
        ) {
            decks.insert(id, name.to_owned());
        }
    }
    Ok(decks)
}

fn collection_offset(connection: &Connection, conf_json: &str) -> FixedOffset {
    let legacy_minutes = serde_json::from_str::<Value>(conf_json)
        .ok()
        .and_then(|value| value.get("creationOffset")?.as_i64())
        .filter(|minutes| (-1_440..=1_440).contains(minutes));
    let normalized_minutes = connection
        .query_row(
            "SELECT val FROM config WHERE key = 'creationOffset'",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .ok()
        .and_then(|value| serde_json::from_slice::<Value>(&value).ok())
        .and_then(|value| value.as_i64())
        .filter(|minutes| (-1_440..=1_440).contains(minutes))
        .or(legacy_minutes)
        .unwrap_or(0);
    let minutes = normalized_minutes;
    let seconds = i32::try_from(-minutes * 60).unwrap_or(0);
    FixedOffset::east_opt(seconds).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap())
}

fn read_reviews(
    connection: &Connection,
    offset: FixedOffset,
) -> Result<(Reviews, ImportErrors), String> {
    let mut statement = connection
        .prepare("SELECT id, cid, ease FROM revlog ORDER BY id")
        .map_err(|error| format!("Anki scheduling history cannot be queried: {error}"))?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("Anki scheduling history cannot be read: {error}"))?;
    let mut reviews: Reviews = HashMap::new();
    let mut errors = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("Anki scheduling row cannot be read: {error}"))?
    {
        let id: i64 = row
            .get(0)
            .map_err(|error| format!("Anki scheduling timestamp cannot be read: {error}"))?;
        let card_id: i64 = row
            .get(1)
            .map_err(|error| format!("Anki scheduling card ID cannot be read: {error}"))?;
        let ease: i64 = row
            .get(2)
            .map_err(|error| format!("Anki scheduling rating cannot be read: {error}"))?;
        if !(1..=4).contains(&ease) {
            continue;
        }
        let Some(timestamp) = DateTime::<Utc>::from_timestamp_millis(id) else {
            errors.push(format!(
                "card {card_id}: review timestamp {id} is outside the supported range"
            ));
            continue;
        };
        reviews.entry(card_id).or_default().push(CardEvent {
            date: timestamp.with_timezone(&offset).date_naive(),
            raw_rating: ease as u8,
            source_line: 0,
        });
    }
    Ok((reviews, errors))
}

fn read_cards(connection: &Connection) -> Result<Vec<AnkiCard>, String> {
    let mut statement = connection
        .prepare(
            "SELECT c.id, CASE WHEN c.odid != 0 THEN c.odid ELSE c.did END, \
             c.ord, n.mid, n.flds FROM cards c JOIN notes n ON n.id = c.nid ORDER BY c.id",
        )
        .map_err(|error| format!("Anki cards cannot be queried: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            let ordinal: i64 = row.get(2)?;
            Ok(AnkiCard {
                id: row.get(0)?,
                deck_id: row.get(1)?,
                ordinal: usize::try_from(ordinal).unwrap_or(0),
                model_id: row.get(3)?,
                fields: row
                    .get::<_, String>(4)?
                    .split('\u{1f}')
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .map_err(|error| format!("Anki cards cannot be read: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Anki card cannot be read: {error}"))
}

fn read_media_manifest(
    archive: &mut ZipArchive<File>,
    archive_path: &Path,
    format: PackageFormat,
) -> Result<BTreeMap<String, String>, String> {
    let mut entry = match archive.by_name("media") {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(format!(
                "{}: cannot read media manifest: {error}",
                archive_path.display()
            ));
        }
    };
    if format == PackageFormat::Latest {
        let mut decoded = Vec::new();
        zstd::stream::copy_decode(entry, &mut decoded).map_err(|error| {
            format!(
                "{}: cannot decompress media manifest: {error}",
                archive_path.display()
            )
        })?;
        let entries = AnkiMediaEntries::decode(decoded.as_slice()).map_err(|error| {
            format!(
                "{}: invalid protobuf media manifest: {error}",
                archive_path.display()
            )
        })?;
        return Ok(entries
            .entries
            .into_iter()
            .enumerate()
            .map(|(index, entry)| {
                (
                    entry
                        .legacy_zip_filename
                        .map_or_else(|| index.to_string(), |index| index.to_string()),
                    entry.name,
                )
            })
            .collect());
    }

    let mut source = String::new();
    entry.read_to_string(&mut source).map_err(|error| {
        format!(
            "{}: media manifest is not valid UTF-8 JSON: {error}",
            archive_path.display()
        )
    })?;
    let values: Value = serde_json::from_str(&source).map_err(|error| {
        format!(
            "{}: invalid media manifest: {error}",
            archive_path.display()
        )
    })?;
    Ok(values
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(entry, name)| Some((entry.clone(), name.as_str()?.to_owned())))
        .collect())
}

fn import_media(
    archive: &mut ZipArchive<File>,
    media: &BTreeMap<String, String>,
    images: &Path,
    format: PackageFormat,
    report: &mut ImportReport,
) -> HashMap<String, String> {
    let mut paths = HashMap::new();
    for (entry_name, original_name) in media {
        let filename = media_filename(original_name);
        paths.insert(original_name.clone(), format!("./images/{filename}"));
        let destination = images.join(&filename);
        let mut entry = match archive.by_name(entry_name) {
            Ok(entry) => entry,
            Err(error) => {
                report.errors.push(format!(
                    "media {original_name:?} ({entry_name}): cannot read archive entry: {error}"
                ));
                continue;
            }
        };
        let outcome = if format == PackageFormat::Latest {
            zstd::stream::read::Decoder::new(entry)
                .map_err(|error| {
                    format!(
                        "{}: cannot start zstd decompression: {error}",
                        destination.display()
                    )
                })
                .and_then(|mut decoder| atomic_create_from_reader(&destination, &mut decoder))
        } else {
            atomic_create_from_reader(&destination, &mut entry)
        };
        match outcome {
            Ok(CreateOutcome::Created) => {
                report.imported_media += 1;
                report.events.push(format!(
                    "copied media {original_name:?} to images/{filename}"
                ));
            }
            Ok(CreateOutcome::Skipped) => {
                report.skipped_media += 1;
                report.events.push(format!(
                    "skipped media images/{filename} ({original_name:?}): already exists"
                ));
            }
            Err(error) => report
                .errors
                .push(format!("media {original_name:?}: {error}")),
        }
    }
    paths
}

fn media_filename(original_name: &str) -> String {
    let stem = stable_name("media", original_name);
    match safe_extension(original_name) {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem,
    }
}

fn safe_extension(name: &str) -> Option<String> {
    let basename = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let extension = basename.rsplit_once('.')?.1;
    if extension.is_empty()
        || extension.len() > 10
        || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}

fn stable_name(kind: &str, identity: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"retent\0");
    digest.update(kind.as_bytes());
    digest.update(b"\0");
    digest.update(identity.as_bytes());
    digest.finalize()[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn render_card(
    card: &AnkiCard,
    collection: &Collection,
    media_paths: &HashMap<String, String>,
) -> String {
    let model = collection.models.get(&card.model_id);
    let fields: HashMap<&str, &str> = model
        .into_iter()
        .flat_map(|model| model.fields.iter().zip(card.fields.iter()))
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let template = model.and_then(|model| {
        model
            .templates
            .iter()
            .find(|template| template.ordinal == card.ordinal)
            .or_else(|| model.templates.get(card.ordinal))
            .or_else(|| model.templates.first())
    });

    let (front_html, mut back_html) = match template {
        Some(template) => {
            let front = render_template(&template.question, &fields, card.ordinal, false, "");
            let answer = render_template(&template.answer, &fields, card.ordinal, true, &front);
            (front, answer)
        }
        None => fallback_faces(&card.fields),
    };
    if let Some(separator) = ANSWER_SEPARATOR.find(&back_html) {
        back_html = back_html[separator.end()..].to_owned();
    } else if let Some(remainder) = back_html.strip_prefix(&front_html) {
        back_html = remainder.to_owned();
    }
    let front = html_to_markdown(&rewrite_media(&front_html, media_paths));
    let back = html_to_markdown(&rewrite_media(&back_html, media_paths));
    let tags = deck_tags(collection.decks.get(&card.deck_id));
    let history = collection
        .reviews
        .get(&card.id)
        .map(|events| render_card_history(events))
        .unwrap_or_default();

    let mut source = format!(
        "---\ntype: card\npriority: 5\ntags: {}\n---\n\n## Front\n\n{}\n\n## Back\n\n{}\n",
        serde_json::to_string(&tags).expect("deck tags serialize"),
        nonempty_face(front),
        nonempty_face(back),
    );
    if !history.is_empty() {
        source.push('\n');
        source.push_str(&history);
    }
    source
}

fn fallback_faces(fields: &[String]) -> (String, String) {
    let front = fields.first().cloned().unwrap_or_default();
    let back = fields.get(1..).unwrap_or_default().join("<br>");
    (front, back)
}

fn render_template(
    template: &str,
    fields: &HashMap<&str, &str>,
    ordinal: usize,
    answer: bool,
    front_side: &str,
) -> String {
    let mut rendered = template.to_owned();
    for (name, value) in fields {
        rendered = render_conditionals(&rendered, name, !strip_html(value).trim().is_empty());
    }
    TEMPLATE_TOKEN
        .replace_all(&rendered, |captures: &Captures<'_>| {
            let token = captures.get(1).map_or("", |value| value.as_str()).trim();
            if token == "FrontSide" {
                return front_side.to_owned();
            }
            let (filter, field_name) = token
                .split_once(':')
                .map_or(("", token), |(filter, field)| (filter.trim(), field.trim()));
            let Some(value) = fields.get(field_name) else {
                return String::new();
            };
            match filter {
                "cloze" => render_clozes(value, ordinal + 1, answer),
                "text" => strip_html(value),
                "type" if !answer => String::new(),
                "type" | "hint" | "" => (*value).to_owned(),
                _ => (*value).to_owned(),
            }
        })
        .into_owned()
}

fn render_conditionals(source: &str, field: &str, present: bool) -> String {
    let mut rendered = source.to_owned();
    for (opening, keep) in [
        (format!("{{{{#{field}}}}}"), present),
        (format!("{{{{^{field}}}}}"), !present),
    ] {
        let closing = format!("{{{{/{field}}}}}");
        while let Some(start) = rendered.find(&opening) {
            let body_start = start + opening.len();
            let Some(relative_end) = rendered[body_start..].find(&closing) else {
                break;
            };
            let end = body_start + relative_end;
            let replacement = if keep { &rendered[body_start..end] } else { "" }.to_owned();
            rendered.replace_range(start..end + closing.len(), &replacement);
        }
    }
    rendered
}

fn render_clozes(source: &str, wanted: usize, answer: bool) -> String {
    CLOZE
        .replace_all(source, |captures: &Captures<'_>| {
            let ordinal = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            let text = captures.get(2).map_or("", |value| value.as_str());
            let hint = captures.get(3).map_or("...", |value| value.as_str());
            if !answer && ordinal == wanted {
                format!("[{hint}]")
            } else {
                text.to_owned()
            }
        })
        .into_owned()
}

fn rewrite_media(source: &str, media_paths: &HashMap<String, String>) -> String {
    let mut entries: Vec<_> = media_paths.iter().collect();
    entries.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    let mut rewritten = source.to_owned();
    for (name, path) in entries {
        rewritten = rewritten.replace(name, path);
        rewritten = rewritten.replace(&name.replace(' ', "%20"), path);
    }
    SOUND
        .replace_all(&rewritten, |captures: &Captures<'_>| {
            format!("<a href=\"{}\">Audio</a>", &captures[1])
        })
        .into_owned()
}

fn html_to_markdown(source: &str) -> String {
    let without_unsafe = SCRIPT_OR_STYLE.replace_all(source, "");
    let markdown = html2md::parse_html(&without_unsafe);
    close_open_fence(normalize_markdown(
        &LEFTOVER_HTML.replace_all(&markdown, ""),
    ))
}

fn strip_html(source: &str) -> String {
    html_to_markdown(source)
}

fn normalize_markdown(source: &str) -> String {
    let mut output = String::new();
    let mut blank = false;
    for line in source.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            if !output.is_empty() && !blank {
                output.push('\n');
            }
            blank = true;
        } else {
            if !output.is_empty() && !output.ends_with('\n') {
                output.push('\n');
            }
            output.push_str(line.trim_start());
            output.push('\n');
            blank = false;
        }
    }
    output.trim().to_owned()
}

fn close_open_fence(mut source: String) -> String {
    let mut fence: Option<(u8, usize)> = None;
    for line in source.lines() {
        let trimmed = line.trim_start();
        match fence {
            Some((character, length))
                if trimmed
                    .bytes()
                    .take_while(|byte| *byte == character)
                    .count()
                    >= length =>
            {
                fence = None;
            }
            None => {
                for &character in b"`~" {
                    let length = trimmed
                        .bytes()
                        .take_while(|byte| *byte == character)
                        .count();
                    if length >= 3 {
                        fence = Some((character, length));
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    if let Some((character, length)) = fence {
        if !source.ends_with('\n') {
            source.push('\n');
        }
        source.extend(std::iter::repeat_n(char::from(character), length));
    }
    source
}

fn nonempty_face(face: String) -> String {
    if face.trim().is_empty() {
        "(empty)".to_owned()
    } else {
        face
    }
}

fn deck_tags(deck_name: Option<&String>) -> Vec<String> {
    let mut tags = Vec::new();
    for component in deck_name
        .into_iter()
        .flat_map(|name| name.split("::"))
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if !tags.iter().any(|tag| tag == component) {
            tags.push(component.to_owned());
        }
    }
    tags
}

fn atomic_create(path: &Path, bytes: &[u8]) -> Result<CreateOutcome, String> {
    atomic_create_with(path, |file| file.write_all(bytes))
}

fn atomic_create_from_reader(path: &Path, reader: &mut impl Read) -> Result<CreateOutcome, String> {
    atomic_create_with(path, |file| std::io::copy(reader, file).map(|_| ()))
}

fn atomic_create_with(
    path: &Path,
    write: impl FnOnce(&mut File) -> std::io::Result<()>,
) -> Result<CreateOutcome, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            return Ok(CreateOutcome::Skipped);
        }
        Ok(_) => {
            return Err(format!(
                "{}: destination exists but is not a regular file",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "{}: cannot inspect destination: {error}",
                path.display()
            ));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("{}: destination has no parent", path.display()))?;
    let mut temporary = Builder::new()
        .prefix(".retent-import-")
        .tempfile_in(parent)
        .map_err(|error| format!("{}: cannot create temporary file: {error}", path.display()))?;
    write(temporary.as_file_mut())
        .and_then(|()| temporary.as_file_mut().flush())
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("{}: cannot write temporary file: {error}", path.display()))?;
    match temporary.persist_noclobber(path) {
        Ok(_) => Ok(CreateOutcome::Created),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(CreateOutcome::Skipped)
        }
        Err(error) => Err(format!(
            "{}: cannot atomically create destination: {}",
            path.display(),
            error.error
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_names_are_uuid_shaped_without_hyphens() {
        let name = stable_name("card", "123");
        assert_eq!(name.len(), 32);
        assert!(name.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert_eq!(name, stable_name("card", "123"));
        assert_ne!(name, stable_name("card", "124"));
    }

    #[test]
    fn renders_clozes_and_nested_deck_tags() {
        assert_eq!(
            render_clozes("A {{c1::one::number}} B {{c2::two}}", 1, false),
            "A [number] B two"
        );
        assert_eq!(render_clozes("A {{c1::one::number}}", 1, true), "A one");
        assert_eq!(
            deck_tags(Some(&"Languages::French::Verbs::French".to_owned())),
            ["Languages", "French", "Verbs"]
        );
    }

    #[test]
    fn strips_html_but_retains_markdown_images() {
        let converted =
            html_to_markdown("<div>Hello <b>there</b><br><img src=\"./images/abc.png\"></div>");
        assert!(!converted.contains('<'));
        assert!(converted.contains("Hello **there**"));
        assert!(converted.contains("![](./images/abc.png)"));
    }

    #[test]
    fn closes_a_fence_left_open_by_malformed_anki_html() {
        assert_eq!(
            close_open_fence("```rust\nfn main() {}".to_owned()),
            "```rust\nfn main() {}\n```"
        );
        assert_eq!(close_open_fence("```\nx\n```".to_owned()), "```\nx\n```");
    }

    #[test]
    fn media_extensions_are_safe() {
        assert_eq!(safe_extension("folder/a.JPG"), Some("jpg".to_owned()));
        assert_eq!(safe_extension("a.bad-name"), None);
        assert_eq!(safe_extension("a"), None);
    }
}
