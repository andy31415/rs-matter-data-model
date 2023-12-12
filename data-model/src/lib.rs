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