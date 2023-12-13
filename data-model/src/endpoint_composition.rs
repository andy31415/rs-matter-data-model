// Represents a specific device type
#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct DeviceType {
    pub name: String,
    pub code: u64,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum DefaultAttributeValue {
    Number(u64),
    Signed(i64),
    String(String),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub enum AttributeHandlingType {
    #[default]
    Ram,
    Callback,
    Persist,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct AttributeInstantiation {
    pub handle_type: AttributeHandlingType,
    pub name: String,
    pub default: Option<DefaultAttributeValue>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct ClusterInstantiation {
    pub name: String,
    pub attributes: Vec<AttributeInstantiation>,
    pub commands: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default)]
pub struct Endpoint {
    pub id: u64,
    pub device_types: Vec<DeviceType>,
    pub bindings: Vec<String>,
    pub instantiations: Vec<ClusterInstantiation>,
}
