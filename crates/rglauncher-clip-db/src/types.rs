use content_inspector::ContentType;
use rusqlite::Row;

#[derive(Debug, Clone, Default)]
pub struct ClipboardEntry {
    pub id: u64,
    pub content: Vec<u8>,
    pub content_size: usize,
    pub last_updated: u64,
    pub mimetype: Option<String>,
    pub extra_preview_data: Option<String>,
    pub content_type: Option<ContentType>,
}

#[derive(Debug, Clone)]
pub struct SearchParams {
    pub query: Option<String>,
    pub use_regex: bool,
    pub offset: usize,
    pub limit: usize,
}

impl Default for SearchParams {
    fn default() -> Self {
        Self {
            query: None,
            use_regex: false,
            offset: 0,
            limit: 100,
        }
    }
}

impl<'stmt> TryFrom<&Row<'stmt>> for ClipboardEntry {
    type Error = rusqlite::Error;
    fn try_from(row: &Row) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: row.get(0)?,
            content: row.get("content")?,
            last_updated: row.get("last_updated")?,
            mimetype: row.get("mimetype")?,
            extra_preview_data: row.get("extra_preview_data")?,
            content_size: row.get("content_size").unwrap_or_default(),
            content_type: row
                .get::<&str, Option<u8>>("content_type")?
                .map(|n| match n {
                    1 => ContentType::UTF_8,
                    2 => ContentType::UTF_8_BOM,
                    3 => ContentType::UTF_16LE,
                    4 => ContentType::UTF_16BE,
                    5 => ContentType::UTF_32LE,
                    6 => ContentType::UTF_32BE,
                    _ => ContentType::BINARY,
                }),
        })
    }
}

impl PartialEq for ClipboardEntry {
    fn eq(&self, other: &Self) -> bool {
        self.content_size == other.content_size
            && self.content_type == other.content_type
            && self.content == other.content
    }
}
impl Eq for ClipboardEntry {}

impl Ord for ClipboardEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.last_updated.cmp(&other.last_updated)
    }
}

impl PartialOrd for ClipboardEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl AsRef<ClipboardEntry> for ClipboardEntry {
    fn as_ref(&self) -> &ClipboardEntry {
        self
    }
}
