/// How mature/usable a member of an API is
///
/// Most things should be stable, however while spec is developed
/// we expect PROVISIONAL to be set.
#[derive(Debug, PartialEq, Copy, Clone, Hash, PartialOrd, Eq, Ord, Default)]
pub enum ApiMaturity {
    #[default]
    STABLE,
    PROVISIONAL,
    INTERNAL,
    DEPRECATED,
}

/// A named numeric value.
///
/// A value that has a name (e.g. enumeration or bitmap constant).
/// May also have an associated maturity that defaults to STABLE
/// while parsing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstantEntry {
    pub maturity: ApiMaturity,
    pub id: String,
    pub code: u64,
}

/// A set of constant entries that correspont to an enumeration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Enum {
    pub doc_comment: Option<String>,
    pub maturity: ApiMaturity,
    pub id: String,
    pub base_type: String,
    pub entries: Vec<ConstantEntry>,
}

/// A set of constant entries that correspont to a bitmap.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bitmap {
    pub doc_comment: Option<String>,
    pub maturity: ApiMaturity,
    pub id: String,
    pub base_type: String,
    pub entries: Vec<ConstantEntry>,
}

/// A generic type such as integers, strings, enums etc.
///
/// Supports information if this is repeated/list as well
/// as a maximum length (if applicable).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DataType {
    pub name: String,
    pub is_list: bool,
    pub max_length: Option<u64>,
}

impl DataType {
    pub fn scalar(name: impl Into<String>) -> DataType {
        DataType {
            name: name.into(),
            is_list: false,
            max_length: None,
        }
    }

    pub fn list_of(name: impl Into<String>) -> DataType {
        DataType {
            name: name.into(),
            is_list: true,
            max_length: None,
        }
    }

    pub fn scalar_of_size(name: impl Into<String>, max_length: u64) -> DataType {
        DataType {
            name: name.into(),
            is_list: false,
            max_length: Some(max_length),
        }
    }
}

/// Represents a generic field.
///
/// Fields have a type, name(id) and numeric code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Field {
    pub data_type: DataType,
    pub id: String,
    pub code: u64,
}

/// Represents a field entry within a struct.
///
/// Specifically this adds structure specific information
/// such as API maturity, optional/nullable/fabric_sensitive
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StructField {
    pub field: Field,
    pub maturity: ApiMaturity,
    pub is_optional: bool,
    pub is_nullable: bool,
    pub is_fabric_sensitive: bool,
}
