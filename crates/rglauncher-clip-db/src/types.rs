use rusqlite::Row;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ContentType {
    Text,
    Image,
    File,
    Link,
    Color,
    Code,
}

impl fmt::Display for ContentType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ContentType::Text => write!(f, "text"),
            ContentType::Image => write!(f, "image"),
            ContentType::File => write!(f, "file"),
            ContentType::Link => write!(f, "link"),
            ContentType::Color => write!(f, "color"),
            ContentType::Code => write!(f, "code"),
        }
    }
}

impl From<String> for ContentType {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "text" => ContentType::Text,
            "image" => ContentType::Image,
            "file" => ContentType::File,
            "link" => ContentType::Link,
            "color" => ContentType::Color,
            "code" => ContentType::Code,
            _ => ContentType::Text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub id: String,
    pub content_type: ContentType,
    pub content: String,
    pub favicon: Option<String>,
    pub timestamp: String,
    pub source: String,
    pub source_icon: Option<String>,
    pub language: Option<String>,
    pub icount: i32,
}

impl ClipboardEntry {
    pub fn new(
        source: String,
        content_type: ContentType,
        content: String,
        favicon: Option<String>,
        source_icon: Option<String>,
        language: Option<String>,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(content_type.to_string().as_bytes());
        hasher.update(content.as_bytes());
        let id = hasher.finalize().to_hex().to_string();

        Self {
            id,
            source,
            source_icon,
            content_type,
            content,
            favicon,
            timestamp: chrono::Utc::now().to_rfc3339(),
            language,
            icount: 0,
        }
    }
}

impl Default for ClipboardEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            content_type: ContentType::Text,
            content: String::new(),
            favicon: None,
            timestamp: String::new(),
            source: "System".to_string(),
            source_icon: None,
            language: None,
            icount: 0,
        }
    }
}

impl<'stmt> TryFrom<&Row<'stmt>> for ClipboardEntry {
    type Error = rusqlite::Error;
    fn try_from(row: &Row) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: row.get("id")?,
            content_type: ContentType::from(row.get::<_, String>("content_type")?),
            content: row.get("content")?,
            favicon: row.get("favicon")?,
            timestamp: row.get("timestamp")?,
            source: row.get("source")?,
            source_icon: row.get("source_icon")?,
            language: row.get("language")?,
            icount: row.get("icount")?,
        })
    }
}

impl PartialEq for ClipboardEntry {
    fn eq(&self, other: &Self) -> bool {
        self.content_type == other.content_type && self.content == other.content
    }
}
impl Eq for ClipboardEntry {}

impl Ord for ClipboardEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.timestamp.cmp(&other.timestamp)
    }
}

impl PartialOrd for ClipboardEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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

impl AsRef<ClipboardEntry> for ClipboardEntry {
    fn as_ref(&self) -> &ClipboardEntry {
        self
    }
}
