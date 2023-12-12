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

/// Defines the type of a structure.
///
/// Response structures contain the underlying code used to send
/// that structure as a reply.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StructType {
    Regular,
    Request,
    Response(u64), // response with a code
}

/// A structure defined in IDL.
///
/// Structures may be regular (as data types), request (used in command inputs)
/// or responses (used as command outputs, have an id)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Struct {
    pub doc_comment: Option<String>,
    pub maturity: ApiMaturity,
    pub struct_type: StructType,
    pub id: String,
    pub fields: Vec<StructField>,
    pub is_fabric_scoped: bool,
}

/// Privilege to access an attribute or invoke a command
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessPrivilege {
    View,
    Operate,
    Manage,
    Administer,
}

/// Priority of a specific event
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EventPriority {
    Critical,
    Info,
    Debug,
}

/// An event that may be emited by a cluster
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Event {
    pub doc_comment: Option<String>,
    pub maturity: ApiMaturity,
    pub priority: EventPriority,
    pub access: AccessPrivilege,
    pub id: String,
    pub code: u64,
    pub fields: Vec<StructField>,
    pub is_fabric_sensitive: bool,
}

/// A command that can be executed on a cluster
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Command {
    pub doc_comment: Option<String>,
    pub maturity: ApiMaturity,
    pub access: AccessPrivilege, // invoke access privilege
    pub id: String,
    pub input: Option<String>,
    pub output: String,
    pub code: u64,
    pub is_timed: bool,
    pub is_fabric_scoped: bool,
}

impl Default for Command {
    fn default() -> Self {
        Self {
            access: AccessPrivilege::Operate,
            doc_comment: None,
            maturity: ApiMaturity::STABLE,
            id: "".into(),
            input: None,
            output: "DefaultSuccess".into(),
            code: 0,
            is_timed: false,
            is_fabric_scoped: false,
        }
    }
}
